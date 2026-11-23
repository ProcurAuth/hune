// Browser case B213 (9.20, 4.18; wave H, P09, ID-K326): the drawer "Inspect value" of a table cell, as measured on Grafana 12.4.2.
// The button shows on hover and on focus, a click or the keys open the drawer, the text goes in as text, Escape gives the focus back,
// and each target is 24 px at least (the rule of B153). All SQL is synthetic.
import { doc, panel, section } from "../lib/docs.mjs";
import { reveal, sleep } from "../lib/h.mjs";

const UID = "inspect-value-test";
const EVIL = "<img src=x onerror=alert(1)>";
// A JSON value of 400 objects (more than 10,000 characters) that ClickHouse builds, the hostile text, a number with no inspect,
// and a number with inspect (alignment right: the button is at the left edge).
const SQL = [
  "SELECT concat('{\"items\":[', arrayStringConcat(arrayMap(i -> concat('{\"id\":', toString(i), ',\"name\":\"item ', toString(i), '\"}'), range(400)), ','), ']}') AS payload,",
  `       '${EVIL}' AS evil, toFloat64(number) AS n, toFloat64(number * 10) AS m`,
  "FROM numbers(3)",
];
const PAYLOAD = `{"items":[${Array.from({ length: 400 }, (_, i) => `{"id":${i},"name":"item ${i}"}`).join(",")}]}`;

const cell = (row, col) => `[data-testid="table-1"] tbody tr[data-row="${row}"] > td:nth-child(${col + 1})`;
const button = (row, col) => `${cell(row, col)} > .cell-inspect`;
const DRAWER = '[data-testid="inspect-value"]';

/** The box of the button of a cell and of the cell, its display and its name. */
const probe = (p, row, col) => p.eval(`(() => {
  const td = document.querySelector(${JSON.stringify(cell(row, col))});
  const b = td.querySelector(".cell-inspect");
  if (!b) return null;
  const r = b.getBoundingClientRect();
  const c = td.getBoundingClientRect();
  return { display: getComputedStyle(b).display, name: b.getAttribute("aria-label"), w: r.width, h: r.height, left: r.left - c.left, right: c.right - r.right, top: r.top - c.top, cellH: c.height };
})()`);

/** The drawer: its title, subtitle, text, children of the block, the boxes of its targets and the active element. */
const drawer = (p) => p.eval(`(() => {
  const d = document.querySelector(${JSON.stringify(DRAWER)});
  if (!d) return null;
  const pre = d.querySelector('[data-testid="inspect-value-text"]');
  const box = d.getBoundingClientRect();
  return {
    title: d.querySelector(".dialog__title").textContent,
    subtitle: d.querySelector(".dialog__subtitle").textContent,
    text: pre.textContent,
    elements: pre.children.length,
    images: d.querySelectorAll("img").length,
    drawer: d.classList.contains("dialog--drawer"),
    right: Math.round(window.innerWidth - box.right),
    width: Math.round(box.width),
    note: !!d.querySelector(".inspect__meta"),
    targets: [...d.querySelectorAll("button")].map((b) => { const r = b.getBoundingClientRect(); return { name: b.getAttribute("aria-label") || b.textContent.trim(), w: r.width, h: r.height }; }),
  };
})()`);

const active = (p) => p.eval(`(() => {
  const a = document.activeElement;
  const td = a.closest("td");
  const tr = a.closest("tr[data-row]");
  return { tag: a.tagName, inspect: a.classList.contains("cell-inspect"), row: tr ? tr.dataset.row : null, col: td ? [...td.parentElement.children].indexOf(td) : -1 };
})()`);

export const cases = [
  {
    id: "B213",
    name: "table inspect: the button on hover and focus, the drawer with the JSON indent and the hostile text, the keys, 24 px targets",
    async run(t) {
      const d = doc(UID, "Inspect value", [
        section(null, [
          panel(1, "table", "Inspect", SQL, {
            grid: { x: 0, y: 0, w: 24, h: 9 },
            fields: { defaults: {}, overrides: [{ match: { by: "names", names: ["payload", "evil", "m"], mode: "include" }, set: { inspect: true } }] },
          }),
        ]),
      ]);
      const have = await t.api.dashboard(UID);
      if (have.status !== 200) {
        const r = await t.api.create("transit", d);
        t.assert(r.status === 201, `create ${UID}: ${r.status} ${r.text.slice(0, 400)}`);
      }
      const p = await t.open(t.dash(UID), { width: 1400, height: 900 });
      await reveal(p, [1]);
      await p.waitSel(cell(2, 3));
      await p.eval(`window.alert = () => { window.__alerted = true; }`);
      await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 2, y: 2 });
      await sleep(100);

      // 1. The buttons: one in each cell of a column with `inspect`, none in the column `n`. They hide while no pointer and no focus is on the cell.
      const counts = await p.eval(`[0, 1, 2, 3].map((c) => document.querySelectorAll('[data-testid="table-1"] tbody tr[data-row] > td:nth-child(' + (c + 1) + ') > .cell-inspect').length)`);
      t.eq(counts, [3, 3, 0, 3], "the buttons of the columns payload, evil, n and m");
      for (const col of [0, 1, 3]) t.eq((await probe(p, 0, col)).display, "none", `the button of column ${col} hides with no hover and no focus`);

      // 2. Hover: the button shows at the right edge, with its name and a target of 24 px at least. A cell aligned right has it at the left edge.
      await p.hover(cell(0, 0));
      await sleep(100);
      const b0 = await probe(p, 0, 0);
      t.eq([b0.display, b0.name], ["flex", "Inspect value"], "the button on hover");
      t.assert(b0.w >= 24 && b0.h >= 24, `the target is 24 px at least: ${b0.w} x ${b0.h}`);
      t.assert(Math.abs(b0.right) <= 1, `the button sits at the right edge: ${b0.right}`);
      await p.hover(cell(0, 3));
      await sleep(100);
      const b3 = await probe(p, 0, 3);
      t.eq(b3.display, "flex", "the button of the number column on hover");
      t.assert(Math.abs(b3.left) <= 1, `a cell aligned right has the button at the left edge: ${b3.left}`);
      t.eq((await probe(p, 0, 0)).display, "none", "the button of a cell with no hover hides again");

      // 3. A click opens the drawer with the JSON text and an indent of 2 spaces. The block holds one text and no element.
      await p.hover(cell(0, 0));
      await p.click(button(0, 0));
      await p.waitSel(DRAWER);
      const j = await drawer(p);
      t.eq([j.title, j.subtitle, j.drawer, j.right], ["Inspect value", "payload", true, 0], "the drawer at the right edge, its title and subtitle");
      t.assert(Math.abs(j.width - 700) <= 1, `the drawer is half of the window wide: ${j.width}`);
      t.assert(PAYLOAD.length > 10000, `the value has ${PAYLOAD.length} characters`);
      t.eq(j.text, JSON.stringify(JSON.parse(PAYLOAD), null, 2), "the JSON text with an indent of 2 spaces");
      t.eq([j.elements, j.images, j.note], [0, 0, false], "one text, no element, no note");
      for (const target of j.targets) t.assert(target.w >= 24 && target.h >= 24, `the target ${target.name} is 24 px at least: ${target.w} x ${target.h}`);
      t.eq(j.targets.map((x) => x.name), ["Close", "Copy to Clipboard", "Close"], "the targets of the drawer");

      // 4. "Copy to Clipboard" writes the shown text and keeps the drawer open.
      const origin = new URL(t.url()).origin;
      await p.browser.send("Browser.grantPermissions", { origin, permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"], browserContextId: p.context.id });
      await p.eval(`navigator.clipboard.writeText("")`);
      await p.clickText(`${DRAWER} .dialog__footer .btn`, "Copy to Clipboard");
      await p.waitFor(`navigator.clipboard.readText().then((t) => t.length > 0)`, { what: "the clipboard" });
      t.eq(await p.eval(`navigator.clipboard.readText()`), j.text, "the clipboard holds the shown text");
      t.assert(await p.exists(DRAWER), "the copy keeps the drawer open");

      // 5. Escape closes the drawer, and the focus goes back to the button.
      await p.press("Escape");
      await p.waitGone(DRAWER);
      t.eq(await active(p), { tag: "BUTTON", inspect: true, row: "0", col: 0 }, "the focus is on the button again");

      // 6. Keys: a focused cell shows its button, Tab reaches it, Enter opens the drawer with the hostile text as text.
      await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 2, y: 2 });
      await p.eval(`(() => { const td = document.querySelector(${JSON.stringify(cell(1, 1))}); td.tabIndex = -1; td.focus(); })()`);
      await sleep(100);
      t.eq((await probe(p, 1, 1)).display, "flex", "the button shows on the focus of the cell");
      await p.press("Tab");
      t.eq(await active(p), { tag: "BUTTON", inspect: true, row: "1", col: 1 }, "Tab on a focused cell reaches its button");
      await p.press("Enter");
      await p.waitSel(DRAWER);
      const e = await drawer(p);
      t.eq([e.subtitle, e.text, e.elements, e.images], ["evil", EVIL, 0, 0], "the hostile text is text: no element comes from it");
      t.eq(await p.eval(`document.querySelectorAll("img").length`), 0, "no image in the page");
      await p.press("Escape");
      await p.waitGone(DRAWER);
      t.eq(await active(p), { tag: "BUTTON", inspect: true, row: "1", col: 1 }, "Escape gives the focus back to the button");

      // 7. Enter on a focused cell opens the drawer too, and Escape gives the focus back to the cell. The arrows move to the next cell.
      await p.eval(`(() => { const td = document.querySelector(${JSON.stringify(cell(2, 0))}); td.tabIndex = -1; td.focus(); })()`);
      await p.press("Enter");
      await p.waitSel(DRAWER);
      t.eq((await drawer(p)).subtitle, "payload", "Enter on a focused cell opens its drawer");
      await p.press("Escape");
      await p.waitGone(DRAWER);
      t.eq(await active(p), { tag: "TD", inspect: false, row: "2", col: 0 }, "Escape gives the focus back to the cell");
      await p.press("ArrowRight");
      t.eq(await active(p), { tag: "TD", inspect: false, row: "2", col: 1 }, "the arrows still move from cell to cell");
      t.eq((await probe(p, 2, 1)).display, "flex", "the next cell shows its button on focus");
      // A cell of the column with no inspect: Enter opens nothing.
      await p.press("ArrowRight");
      await p.press("Enter");
      await sleep(150);
      t.assert(!(await p.exists(DRAWER)), "Enter on a cell with no button opens nothing");
      t.assert(!(await p.eval(`!!window.__alerted`)), "no script ran");
    },
  },
];
