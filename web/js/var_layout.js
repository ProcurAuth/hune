// The placement of a variable in the grid (4.7a, 9.10). Pure module: no DOM access.
// A variable with a member `placement` has its control in the grid of a section. The page draws it with the frame of a panel: for the
// grid and for the drag code, each placement is a "placed panel" with a negative id. The document never holds such a panel.

/** The type of a placed panel. It is a member of the grid only, never of a document. */
export const PLACED_TYPE = "variable";

/** The name of the variable of a placed panel, or the empty text. */
export const variableNameOf = (panel) => (panel && panel.options && typeof panel.options.variable === "string" ? panel.options.variable : "");

/** The label of a variable: its `label`, else its `name` (9.10). */
export const labelOf = (variable) => variable.label || variable.name;

/** The id of the placed panel of the variable at `index` in the array `variables`: -1, -2, ... (never an id of a panel). */
export const placedId = (index) => -(index + 1);

/** The placed panel of one variable. The title is `placement.title`, else the empty text (the control shows its label chip). */
export function placedPanel(variable, index) {
  const p = variable.placement;
  return { id: placedId(index), type: PLACED_TYPE, title: p.title || "", description: "", transparent: false, grid: { x: p.x, y: p.y, w: p.w, h: p.h }, options: { variable: variable.name } };
}

/** The variables that have a placement: `[{ variable, index }]` in document order. */
export function placedVariables(doc) {
  const out = [];
  doc.variables.forEach((variable, index) => {
    if (variable.placement) out.push({ variable, index });
  });
  return out;
}

/** A copy of the document for the grid: each placement is a placed panel in the panels of its section. A section that does not exist gets none. */
export function gridDoc(doc) {
  const placed = placedVariables(doc).filter(({ variable }) => doc.sections[variable.placement.section]);
  if (!placed.length) return doc;
  return {
    ...doc,
    sections: doc.sections.map((section, si) => {
      const mine = placed.filter(({ variable }) => variable.placement.section === si).map(({ variable, index }) => placedPanel(variable, index));
      return mine.length ? { ...section, panels: [...section.panels, ...mine] } : section;
    }),
  };
}

/**
 * Where each control goes (9.10). `variables`: the variables of the page, in order. `slots`: a Map from a name to the list of the
 * slots of that name, as `[{ usable: boolean }]`. A slot is usable when the grid that holds it is shown.
 * The result is a Map from a name to `{ slot: index }` (the first usable slot), `{ bar: true }` or `{ none: true }`.
 * A variable with `hide = "variable"` never goes to the bar.
 */
export function planHomes(variables, slots) {
  const homes = new Map();
  const barable = new Set(variables.filter((v) => v.hide !== "variable").map((v) => v.name));
  const names = new Set([...variables.map((v) => v.name), ...slots.keys()]);
  for (const name of names) {
    const list = slots.get(name) || [];
    const at = list.findIndex((s) => s.usable);
    if (at >= 0) homes.set(name, { slot: at });
    else if (barable.has(name)) homes.set(name, { bar: true });
    else homes.set(name, { none: true });
  }
  return homes;
}

/** The names that the bar shows, in the order of `variables`. */
export function barNames(variables, homes) {
  return variables.filter((v) => homes.get(v.name) && homes.get(v.name).bar).map((v) => v.name);
}

/** The accessible name of a placed control (9.26): its title, else the label of its variable. */
export const groupLabel = (title, variable) => title || labelOf(variable);
