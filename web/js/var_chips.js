// The selected values of a variable as chips (9.10, rule "chips"). Pure module: no DOM access.
// A variable that takes several values (`multi`) shows its selection as chips. A variable that takes one value keeps its text.
// "All" is one chip with no cross. A value has a cross, except the last value: a selection is never empty.

/**
 * The chips for a selection, or null when the variable shows text (it takes one value, or it is not a list).
 * Each chip is { value, text, all, removable }. The text of a value is the text of its first option, else the value.
 */
export function chipsOf(variable, options, selection) {
  if (!variable.multi || (variable.kind !== "query" && variable.kind !== "custom")) return null;
  if (selection.all) return [{ value: "$__all", text: "All", all: true, removable: false }];
  const texts = new Map();
  for (let i = options.length - 1; i >= 0; i--) texts.set(options[i].value, options[i].text);
  const removable = selection.values.length > 1;
  return selection.values.map((value) => ({ value, text: texts.has(value) ? texts.get(value) : value, all: false, removable }));
}

/** The selection without one value, or the same selection when the value cannot go (it is the last one, or it is not selected). */
export function withoutValue(selection, value) {
  if (selection.all || !selection.values.includes(value) || selection.values.length < 2) return selection;
  return { all: false, values: selection.values.filter((v) => v !== value) };
}

/** The selection without its last value (the key Backspace in the empty field), or the same selection when the last value cannot go. */
export function withoutLast(selection) {
  return selection.all || selection.values.length < 2 ? selection : withoutValue(selection, selection.values.at(-1));
}

/**
 * How many chips fit in a row. `widths` are the widths of the chips, `gap` is the space between two chips, `available` is the width of the row and
 * `plusWidth(n)` is the width of the chip "+n" for n hidden chips. At least one chip shows, even when it is wider than the row (its text is cut by
 * the style). When some chips are hidden, the chip "+n" takes its place in the row.
 */
export function fitCount(widths, gap, available, plusWidth) {
  const total = widths.length;
  if (total === 0) return 0;
  const full = widths.reduce((sum, w) => sum + w, 0) + gap * (total - 1);
  if (full <= available) return total;
  let used = 0;
  let count = 0;
  for (let i = 0; i < total; i++) {
    const hidden = total - (i + 1);
    const next = used + (i > 0 ? gap : 0) + widths[i];
    const needed = next + (hidden > 0 ? gap + plusWidth(hidden) : 0);
    if (needed > available) break;
    used = next;
    count = i + 1;
  }
  return Math.max(1, count);
}
