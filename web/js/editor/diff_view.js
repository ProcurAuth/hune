// The display of a line diff (10.15, 10.10, 10.11). The text of a line goes in with textContent only.

import { badge, button, el } from "../ui.js";
import { diffLines } from "./diff.js";
import { saveTextFile } from "../list.js";

// A header row for the screen readers: the columns of a diff line (9.26).
const head = () => el("thead", { class: "visually-hidden" }, el("tr", {}, ...["Old line", "New line", "Change", "Text"].map((t) => el("th", { scope: "col", text: t }))));

// Builds the diff view. `a` and `b` are { text, name } (the file name of the download when the diff is too large).
export function renderDiff(a, b, { title = null } = {}) {
  const result = diffLines(a.text, b.text);
  const root = el("div", { class: "diff", dataset: { testid: "diff-view" } });
  let added = 0;
  let removed = 0;
  for (const hunk of result.hunks || []) for (const l of hunk.lines) { if (l.op === "+") added += 1; else if (l.op === "-") removed += 1; }
  if (title) root.append(el("div", { class: "diff__head" }, el("h3", { class: "diff__title", text: title }),
    ...(result.tooLarge || !result.hunks.length ? [] : [badge(`${added} added`, "ok"), badge(`${removed} removed`, "error")])));
  if (result.tooLarge) {
    root.append(
      el("p", { text: "The two versions differ too much to show the changes." }),
      el("div", { class: "diff__downloads" },
        button(`Download ${a.name}`, { on: { click: () => saveTextFile(a.name, a.text) } }, "download"),
        button(`Download ${b.name}`, { on: { click: () => saveTextFile(b.name, b.text) } }, "download")),
    );
    return { el: root, result };
  }
  if (!result.hunks.length) {
    root.append(el("p", { class: "diff__empty", text: "There is no change." }));
    return { el: root, result };
  }
  for (const hunk of result.hunks) {
    const rows = hunk.lines.map((l) =>
      el("tr", { class: `diff__line diff__line--${l.op === "+" ? "add" : l.op === "-" ? "del" : "same"}` },
        el("td", { class: "diff__no", text: l.aNo === null ? "" : String(l.aNo), "aria-hidden": "true" }),
        el("td", { class: "diff__no", text: l.bNo === null ? "" : String(l.bNo), "aria-hidden": "true" }),
        el("td", { class: "diff__sign", text: l.op === " " ? " " : l.op, "aria-label": l.op === "+" ? "Added" : l.op === "-" ? "Removed" : "Unchanged" }),
        el("td", { class: "diff__text" }, el("pre", { text: l.text }))));
    root.append(el("div", { class: "diff__hunk" }, el("div", { class: "diff__header", text: hunk.header }), el("table", { class: "diff__table" }, head(), el("tbody", {}, ...rows))));
  }
  return { el: root, result };
}
