// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B240 (10.4): the JSON view of the edit mode has colours (4.5:1 in both themes), marks in the gutter for the lines that differ from the
// saved dashboard (added, modified, a border where lines are gone, a band under the number of a syntax error), a revert that clears them, a height
// that fills the page at four window sizes, and the caps of the colour layer.
import { enterEdit, makeSeed, moreItem } from "../lib/ed.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";

const UID = "jv40";
const SIZES = [[1440, 900], [1280, 720], [768, 900], [390, 800]];

// The marks of the gutter as "kind@line" and "kind@linexlines".
const marks = (p) => p.eval(`[...document.querySelectorAll(".jarea__mark")].map((m) => m.dataset.kind + "@" + m.dataset.line + "x" + m.dataset.lines)`);
const text = (p) => p.eval(`document.querySelector('[data-testid="json-text"]').value`);

// The contrast of the colours of the area against its background, and of the bars of the gutter, for one theme.
const CONTRAST = `(theme) => {
  document.documentElement.dataset.theme = theme;
  const parse = (v) => { const m = /^rgba?\\(([^)]*)\\)$/.exec(v); if (m) return m[1].split(/[ ,\\/]+/).filter(Boolean).slice(0, 3).map(Number);
    const c = /^color\\(srgb ([^)]*)\\)$/.exec(v); return c ? c[1].split(" ").slice(0, 3).map((x) => Math.round(Number(x) * 255)) : null; };
  const lum = (c) => { const [r, g, b] = c.map((v) => { const s = v / 255; return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4; }); return 0.2126 * r + 0.7152 * g + 0.0722 * b; };
  const ratio = (a, b) => { const x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };
  const bg = parse(getComputedStyle(document.querySelector(".jframe")).backgroundColor);
  const out = { kinds: {}, marks: {}, numbers: ratio(parse(getComputedStyle(document.querySelector(".jarea__gutter")).color), bg) };
  for (const k of ["jt-key", "jt-str", "jt-num", "jt-bool", "jt-null", "jt-punct"]) {
    const e = document.querySelector("." + k);
    out.kinds[k] = e ? ratio(parse(getComputedStyle(e).color), bg) : null;
  }
  for (const k of ["--success", "--accent-fill", "--danger"]) {
    const probe = document.createElement("i");
    probe.style.cssText = "position:fixed;background:var(" + k + ")";
    document.body.append(probe);
    out.marks[k] = ratio(parse(getComputedStyle(probe).backgroundColor), bg);
    probe.remove();
  }
  return out;
}`;

// The box of the area against the window: the gap under it is the padding of the page, and the page has no scroll of its own.
const FIT = `(() => {
  const f = document.querySelector(".jframe").getBoundingClientRect();
  const pad = parseFloat(getComputedStyle(document.getElementById("app-main")).paddingBottom);
  return { gap: Math.round(window.innerHeight - f.bottom), pad: Math.round(pad), scroll: document.documentElement.scrollHeight - window.innerHeight, height: Math.round(f.height) };
})()`;

async function setSize(p, width, height) {
  await p.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  await sleep(350);
}

export const cases = [
  {
    id: "B240",
    name: "JSON view: colours, marks of the changed lines, revert, full height, caps (10.4)",
    async run(t) {
      await makeSeed(t, UID);
      const p = await t.open(t.dash(UID));
      await waitPanelsReady(p, [1, 2, 3]);
      await enterEdit(p);
      await moreItem(p, "json-tab");
      await p.waitSel(p.tid("json-text"));
      await sleep(600);
      const saved = await text(p);
      const lines = saved.split("\n");

      // 1. Colours: each kind that the seed has is coloured, in both themes with 4.5:1, and the bars of the gutter have 3:1.
      const kinds = await p.eval(`[...new Set([...document.querySelectorAll(".jarea__code span")].map((s) => s.className))]`);
      for (const kind of ["jt-key", "jt-str", "jt-num", "jt-punct"]) t.assert(kinds.includes(kind), `the layer has ${kind}: ${JSON.stringify(kinds)}`);
      const colourOfText = await p.eval(`getComputedStyle(document.querySelector('[data-testid="json-text"]')).color`);
      t.assert(/rgba\(0, 0, 0, 0\)|transparent/.test(colourOfText), `the text of the text area is transparent: ${colourOfText}`);
      for (const theme of ["light", "dark"]) {
        const c = await p.eval(`(${CONTRAST})(${JSON.stringify(theme)})`);
        for (const [kind, ratio] of Object.entries(c.kinds)) if (ratio !== null) t.assert(ratio >= 4.5, `${theme}: ${kind} has ${ratio.toFixed(2)}:1`);
        for (const [token, ratio] of Object.entries(c.marks)) t.assert(ratio >= 3, `${theme}: the bar ${token} has ${ratio.toFixed(2)}:1`);
        t.assert(c.numbers >= 4.5, `${theme}: the line numbers have ${c.numbers.toFixed(2)}:1`);
      }
      await p.eval(`document.documentElement.dataset.theme = "light"; true`);
      // The layer shows the real text: the first row of the layer is the first row of the text area.
      t.eq(await p.eval(`document.querySelector(".jarea__code").textContent.split("\\n")[0]`), lines[0], "the layer draws the first line");

      // 2. An unchanged draft has no mark. The marks follow the lines that differ from the saved dashboard.
      t.eq(await marks(p), [], "no mark for an unchanged draft");
      const set = async (rows) => {
        await p.fill(p.tid("json-text"), rows.join("\n"));
        await sleep(450);
      };
      const edited = [...lines];
      edited[2] = `${edited[2]} `;
      await set(edited);
      t.eq(await marks(p), ["modified@3x1"], "a changed line is modified");
      edited.splice(8, 0, "", "");
      await set(edited);
      t.eq((await marks(p)).sort(), ["added@9x2", "modified@3x1"], "two new lines are added");
      const at = edited.findIndex((l) => l.includes('"links"'));
      t.assert(at > 0, "the seed has a member links");
      edited.splice(at, 1);
      await set(edited);
      const afterDelete = (await marks(p)).sort();
      t.assert(afterDelete.includes(`deleted@${at + 1}x0`), `a mark at the border of the deleted line: ${JSON.stringify(afterDelete)}`);
      t.assert(afterDelete.includes("modified@3x1") && afterDelete.includes("added@9x2"), "the other marks stay");

      // 3. A syntax error marks the number of its line, and the list gives the same line.
      const broken = [...lines];
      broken[1] = broken[1].replace(/,\s*$/, "");
      await set(broken);
      await p.waitFor(`!!document.querySelector(".jarea__mark--error")`, { what: "error mark" });
      const errorMark = await p.eval(`document.querySelector(".jarea__mark--error").dataset.line`);
      const listed = await p.eval(`document.querySelector('[data-testid="json-errors"]').textContent`);
      t.assert(new RegExp(`line ${errorMark},`).test(listed), `the band is on line ${errorMark}: ${listed}`);

      // 4. The height with the list of errors, at four sizes: the area ends one page padding above the edge, and the page does not scroll.
      for (const [w, h] of SIZES) {
        await setSize(p, w, h);
        const fit = await p.eval(FIT);
        t.assert(Math.abs(fit.gap - fit.pad) <= 1, `${w}x${h} with errors: the gap under the area is ${fit.gap}, the padding is ${fit.pad}`);
        t.assert(fit.scroll <= 0, `${w}x${h} with errors: the page scrolls by ${fit.scroll}`);
      }
      await setSize(p, 1440, 900);

      // 5. Revert: the saved text comes back and the marks go.
      await p.click(p.tid("discard-button"));
      await p.waitFor(`[...document.querySelectorAll(".dialog button")].some((b) => b.textContent.trim() === "Discard")`, { what: "the dialog of the revert" });
      await p.eval(`[...document.querySelectorAll(".dialog button")].find((b) => b.textContent.trim() === "Discard").click(); true`);
      await p.waitFor(`document.querySelector('[data-testid="json-text"]').value === ${JSON.stringify(saved)}`, { what: "the saved text after the revert" });
      await sleep(300);
      t.eq(await marks(p), [], "no mark after a revert");

      // 6. The height without the list of errors, at four sizes.
      for (const [w, h] of SIZES) {
        await setSize(p, w, h);
        const fit = await p.eval(FIT);
        t.assert(Math.abs(fit.gap - fit.pad) <= 1, `${w}x${h}: the gap under the area is ${fit.gap}, the padding is ${fit.pad}`);
        t.assert(fit.scroll <= 0, `${w}x${h}: the page scrolls by ${fit.scroll}`);
        t.assert(fit.height >= 200, `${w}x${h}: the area is ${fit.height} px high`);
      }
      await setSize(p, 1440, 900);

      // 7. The caps: a text of 6,000 lines draws a window of rows, and a text of one line has 2,000 coloured parts at most.
      const big = Array.from({ length: 6000 }, (_, i) => `  "k${i}": [${i}, "v", true, null],`).join("\n");
      await p.eval(`(() => { const t = document.querySelector('[data-testid="json-text"]'); t.value = ${JSON.stringify(`{\n${big}\n  "end": 0\n}\n`)}; t.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
      await sleep(500);
      const drawn = await p.eval(`document.querySelector(".jarea__code").textContent.split("\\n").length`);
      t.assert(drawn > 40 && drawn < 500, `a window of rows is drawn, not the 6,000 lines: ${drawn}`);
      await p.eval(`(() => { const t = document.querySelector('[data-testid="json-text"]'); t.scrollTop = 3000 * 18; t.dispatchEvent(new Event("scroll")); return true; })()`);
      await sleep(300);
      const middle = await p.eval(`(() => { const t = document.querySelector('[data-testid="json-text"]'); const row = Math.floor(t.scrollTop / 18); return document.querySelector(".jarea__code").textContent.includes(t.value.split("\\n")[row]); })()`);
      t.assert(middle, "after a jump the layer draws the row at the top");
      await p.eval(`(() => { const t = document.querySelector('[data-testid="json-text"]'); t.value = JSON.stringify({ a: Array.from({ length: 2500 }, (_, i) => ({ id: i, on: true })) }); t.scrollTop = 0; t.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
      await sleep(500);
      t.eq(await p.eval(`document.querySelectorAll(".jarea__code span").length`), 2000, "one line has 2,000 coloured parts at most");
    },
  },
];
