// The thresholds editor and the colour mode editor (10.3a), used by the groups "Thresholds" and "Colour" of the options pane and by
// the properties `thresholds` and `color` of an override. A control writes the draft through `onCommit`, for a valid value only.

import { NAMED } from "../../colors.js";
import { isColorValid } from "../validators.js";
import { button, clear, el, uniqueId } from "../../ui.js";
import { COLOR_MODES, COLOR_MODE_LABELS } from "../props.js";
import { segmentedField, swatches } from "../form.js";
import { COLOR_NAMES_ALL, selectField } from "./controls.js";

// The CSS value of a colour of the document, for a preview mark.
export const cssColor = (c) => (COLOR_NAMES_ALL.includes(c) ? `var(--viz-${c})` : isColorValid(c) ? c : "transparent");

/** A colour with a grid of swatches and a text field. `onValue(colour)` runs for a valid colour. */
export function colorField(label, value, onValue) {
  const text = el("input", { class: "input input--mono", type: "text", spellcheck: "false", value: value || "", "aria-label": `${label} value` });
  const err = el("p", { class: "field-error", role: "alert", hidden: true });
  const grid = swatches(Object.keys(NAMED), value, (name) => {
    text.value = name;
    err.hidden = true;
    text.removeAttribute("aria-invalid");
    grid.set(name);
    onValue(name);
  });
  grid.el.classList.add("swatches--grid");
  text.addEventListener("input", () => {
    if (!isColorValid(text.value)) {
      err.textContent = "A colour is a name, #rgb, #rrggbb, #rrggbbaa, rgb(...) or rgba(...).";
      err.hidden = false;
      text.setAttribute("aria-invalid", "true");
      return;
    }
    err.hidden = true;
    text.removeAttribute("aria-invalid");
    grid.set(text.value);
    onValue(text.value);
  });
  return el("div", { class: "field colorfield" }, el("span", { class: "field__label", text: label }), grid.el, text, err);
}

/**
 * The editor of `color` ({ mode, fixed }): a select of the 14 modes (9.16) and, with `fixed`, the colour field.
 * `value` is the object, or undefined when the member is not set (`fallback` is the mode that the page uses then).
 */
export function colorModeEditor({ value, fallback = "thresholds", onCommit, testid = "color-mode" }) {
  const mode = value ? value.mode : fallback;
  const host = el("div", { class: "fixedcolor" });
  const draw = (m, fixed) => {
    clear(host);
    if (m === "fixed") host.append(colorField("Fixed colour", fixed || "green", (v) => onCommit({ mode: "fixed", fixed: v })));
  };
  const select = selectField({
    label: "Colour mode",
    options: COLOR_MODES.map((m) => [m, COLOR_MODE_LABELS[m]]),
    value: mode,
    testid,
    apply(m) {
      onCommit({ mode: m, fixed: m === "fixed" ? "green" : null });
      draw(m, "green");
    },
  });
  draw(mode, value && value.fixed);
  return el("div", { class: "colormode" }, select.node, host);
}

/**
 * The editor of the thresholds (4.21): the mode and the steps. `value` is the object. `onCommit(thresholds)` runs for a valid object.
 * The first step is the base and has no value.
 */
export function thresholdsEditor(scope, { value, onCommit }) {
  const root = el("div", { class: "thresholds" });
  const stepsBox = el("div", { class: "steps" });
  const thError = el("p", { class: "field-error", role: "alert", hidden: true });
  const listId = uniqueId("color-list");
  const draft = structuredClone(value);
  const show = (text) => {
    thError.textContent = text;
    thError.hidden = !text;
  };
  const commit = (t) => {
    const values = t.steps.slice(1).map((s) => s.value);
    if (values.some((v) => typeof v !== "number" || !Number.isFinite(v))) return show("Each step needs a number.");
    for (let i = 1; i < values.length; i++) if (!(values[i] > values[i - 1])) return show("Step values must go up.");
    if (t.steps.some((s) => !isColorValid(s.color))) return show("A step colour is not valid.");
    show("");
    onCommit(structuredClone(t));
    return undefined;
  };
  const modeSelect = segmentedField("Mode", [{ value: "absolute", label: "Absolute" }, { value: "percentage", label: "Percentage" }], draft.mode, (m) => {
    draft.mode = m;
    commit(draft);
  });
  const drawSteps = () => {
    clear(stepsBox);
    draft.steps.forEach((step, i) => {
      const dot = el("span", { class: "step__dot", "aria-hidden": "true" });
      dot.style.setProperty("--swatch-color", cssColor(step.color));
      const valueInput = el("input", { class: "input input--num", type: "text", inputmode: "decimal", spellcheck: "false", value: step.value === null ? "" : String(step.value), disabled: i === 0, "aria-label": i === 0 ? "Base" : `Step ${i} value`, placeholder: i === 0 ? "Base" : "value" });
      valueInput.addEventListener("input", scope.debounce(() => {
        draft.steps[i].value = valueInput.value.trim() === "" ? NaN : Number(valueInput.value);
        commit(draft);
      }));
      const colorInput = el("input", { class: "input input--mono", type: "text", spellcheck: "false", value: step.color, "aria-label": `Step ${i} colour`, list: listId });
      colorInput.addEventListener("input", scope.debounce(() => {
        draft.steps[i].color = colorInput.value;
        dot.style.setProperty("--swatch-color", cssColor(colorInput.value));
        commit(draft);
      }));
      const rm = i === 0 ? el("span", { class: "step__spacer" }) : button(`Remove step ${i}`, { on: { click: () => {
        scope.flush();
        draft.steps.splice(i, 1);
        drawSteps();
        commit(draft);
      } } }, "close", { iconOnly: true, variant: "ghost", size: "sm" });
      stepsBox.append(el("div", { class: "step" }, dot, valueInput, colorInput, rm));
    });
  };
  drawSteps();
  const addStep = button("Add step", { on: { click: () => {
    scope.flush();
    const last = draft.steps[draft.steps.length - 1];
    draft.steps.push({ value: (typeof last.value === "number" ? last.value : 0) + 10, color: "blue" });
    drawSteps();
    commit(draft);
  } } }, "plus", { size: "sm" });
  const list = el("datalist", { id: listId }, ...COLOR_NAMES_ALL.map((c) => el("option", { value: c })));
  root.append(modeSelect.node, stepsBox, thError, el("div", { class: "ogroup__actions" }, addStep), list);
  return root;
}
