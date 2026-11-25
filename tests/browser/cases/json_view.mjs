// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B240, B270 and B271 (10.4). The row of the area is 20 px (css `--json-row`, script `ROW`).
// Browser case B240 (10.4): the JSON view of the edit mode has colours (4.5:1 in both themes), marks in the gutter for the lines that differ from the
// saved dashboard (added, modified, a border where lines are gone, a band under the number of a syntax error), a revert that clears them, a height
// that fills the page at four window sizes, and the caps of the colour layer.
import { enterEdit, makeSeed, moreItem } from "../lib/ed.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";

const UID = "jv40";
const SIZES = [[1440, 900], [1280, 720], [768, 900], [390, 800]];

// The marks of the gutter as "kind@line" and "kind@linexlines".
const marks = (p) => p.eval(`[...document.querySelectorAll(".jarea__mark")].map((m) => m.dataset.kind + "@" + m.dataset.line + "x" + m.dataset.lines)`);
const text_ = (p) => p.eval(`document.querySelector('[data-testid="json-text"]').value`);

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
  return { gap: Math.round(window.innerHeight - f.bottom), pad: Math.round(pad), scroll: document.getElementById("app-main").scrollHeight - document.getElementById("app-main").clientHeight, height: Math.round(f.height) };
})()`;

async function setSize(p, width, height) {
  await p.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  await sleep(350);
}

// The colour that a custom property gives, as the browser writes it (a probe element takes the property as its background).
const TOKEN = (name) => `(() => { const e = document.createElement("i"); e.style.cssText = "position:fixed;background:var(${name})"; document.body.append(e); const v = getComputedStyle(e).backgroundColor; e.remove(); return v; })()`;

// Makes the seed dashboard and opens its JSON view.
async function openJson(t, uid) {
  await makeSeed(t, uid);
  const p = await t.open(t.dash(uid));
  await waitPanelsReady(p, [1, 2, 3]);
  await enterEdit(p);
  await moreItem(p, "json-tab");
  await p.waitSel(p.tid("json-text"));
  await sleep(600);
  return p;
}

// A real click at a row and a column of the text area (the first row is 0). The width of a column is the width of the first character, "{".
async function clickAt(p, row, col) {
  const at = await p.eval(`(() => { const a = document.querySelector('[data-testid="json-text"]'); const r = a.getBoundingClientRect(); const ch = document.querySelector(".jarea__code span").getBoundingClientRect().width;
    return { x: r.left + 10 + ${col} * ch + 1, y: r.top + 8 + ${row} * 20 + 10 - a.scrollTop }; })()`);
  await p.send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button: "left", clickCount: 1 });
  await p.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: at.x, y: at.y, button: "left", clickCount: 1 });
  await sleep(250);
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
      const saved = await text_(p);
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
      await p.eval(`(() => { const t = document.querySelector('[data-testid="json-text"]'); t.scrollTop = 3000 * 20; t.dispatchEvent(new Event("scroll")); return true; })()`);
      await sleep(300);
      const middle = await p.eval(`(() => { const t = document.querySelector('[data-testid="json-text"]'); const row = Math.floor(t.scrollTop / 20); return document.querySelector(".jarea__code").textContent.includes(t.value.split("\\n")[row]); })()`);
      t.assert(middle, "after a jump the layer draws the row at the top");
      await p.eval(`(() => { const t = document.querySelector('[data-testid="json-text"]'); t.value = JSON.stringify({ a: Array.from({ length: 2500 }, (_, i) => ({ id: i, on: true })) }); t.scrollTop = 0; t.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
      await sleep(500);
      t.eq(await p.eval(`document.querySelectorAll(".jarea__code span").length`), 2000, "one line has 2,000 coloured parts at most");
    },
  },
  {
    id: "B270",
    name: "JSON view: the look of VS Code Light+ and Dark+: colours by kind and depth, surface, selection, gutter, scroll bars (10.4)",
    async run(t) {
      const p = await openJson(t, "jv70");
      const text = await text_(p);
      for (const theme of ["light", "dark"]) {
        await p.eval(`document.documentElement.dataset.theme = ${JSON.stringify(theme)}; true`);
        await sleep(200);
        await p.eval(`(() => { const a = document.querySelector('[data-testid="json-text"]'); a.scrollTop = 0; a.dispatchEvent(new Event("scroll")); return true; })()`);
        await sleep(200);
        // 1. Each kind has the colour of its token. The first { has the first bracket colour, and the [ of the tags (one level deep) the second.
        const kinds = { "jt-key": "--json-key", "jt-str": "--json-str", "jt-num": "--json-num", "jt-bool": "--json-const", "jt-punct": "--json-punct", "jt-b0": "--json-bracket-1", "jt-b1": "--json-bracket-2" };
        for (const [cls, token] of Object.entries(kinds)) {
          const probe = await p.eval(`(() => { const e = document.querySelector(".jarea__code .${cls}"); return e ? [getComputedStyle(e).color, ${TOKEN(token)}] : null; })()`);
          if (probe === null) {
            t.assert(cls === "jt-bool", `${theme}: the layer has a span ${cls}`);
            continue;
          }
          t.eq(probe[0], probe[1], `${theme}: ${cls} has the colour of ${token}`);
        }
        const first = await p.eval(`document.querySelector(".jarea__code span").className`);
        t.eq(first, "jt-b0", `${theme}: the first bracket of the document is the first bracket colour`);
        // 2. The surface, the font and the row: 14 px text and a row of 20 px (1.43), the editor background, no border between the gutter and the text.
        const surface = await p.eval(`(() => { const f = document.querySelector(".jframe"); const g = document.querySelector(".jarea__gutter"); const a = document.querySelector('[data-testid="json-text"]');
          return { bg: getComputedStyle(f).backgroundColor, bgToken: ${TOKEN("--editor-solid-bg")}, size: getComputedStyle(f).fontSize, line: getComputedStyle(f).lineHeight, border: getComputedStyle(g).borderRightWidth,
            gutter: getComputedStyle(g).color, gutterToken: ${TOKEN("--json-lineno")}, tab: getComputedStyle(a).tabSize, fontOk: /mono|Menlo|Consolas/i.test(getComputedStyle(f).fontFamily) }; })()`);
        t.eq(surface.bg, surface.bgToken, `${theme}: the editor has the editor background`);
        t.eq([surface.size, surface.line, surface.border, surface.tab], ["14px", "20px", "0px", "2"], `${theme}: size, row, border and tab of the area`);
        t.eq(surface.gutter, surface.gutterToken, `${theme}: the line numbers have the colour of --json-lineno`);
        t.assert(surface.fontOk, `${theme}: the area has a monospace font`);
        // 3. The selection has the colour of VS Code: the blue with the focus, a paler grey without it. The coloured text stays over it.
        await p.eval(`(() => { const a = document.querySelector('[data-testid="json-text"]'); a.focus(); a.setSelectionRange(0, 40); a.dispatchEvent(new Event("select")); return true; })()`);
        await sleep(150);
        const selection = await p.eval(`[getComputedStyle(document.querySelector('[data-testid="json-text"]'), "::selection").backgroundColor, ${TOKEN("--json-selection")}]`);
        t.eq(selection[0], selection[1], `${theme}: the selection has the colour of --json-selection`);
        const stack = await p.eval(`(() => { const a = document.querySelector('[data-testid="json-text"]'); const r = a.getBoundingClientRect(); const e = document.elementFromPoint(r.left + 40, r.top + 60); return e === a; })()`);
        t.assert(stack, `${theme}: a pointer on the text reaches the text area (the layers take no pointer)`);
        // 4. The scroll bar is 10 px wide, thin as in VS Code.
        const bar = await p.eval(`(() => { const a = document.querySelector('[data-testid="json-text"]'); return a.offsetWidth - a.clientWidth; })()`);
        t.eq(bar, 10, `${theme}: the scroll bar of the area is 10 px wide`);
        await p.eval(`(() => { document.querySelector('[data-testid="json-text"]').blur(); return true; })()`);
      }
      await p.eval(`document.documentElement.dataset.theme = "light"; true`);
      t.assert(text.length > 100, "the document has text");
    },
  },
  {
    id: "B271",
    name: "JSON view: the line of the caret, its number, the bracket and its pair, the guide of the pair, the wavy line of an error (10.4)",
    async run(t) {
      const p = await openJson(t, "jv71");
      await p.eval(`document.documentElement.dataset.theme = "light"; true`);
      const lines = (await text_(p)).split("\n");
      const tags = lines.findIndex((l) => /^\s*"tags": \[$/.test(l));
      const state = () => p.eval(`(() => { const q = (s) => document.querySelector(s); const vis = (e) => !!e && !e.hidden;
        const matches = [...document.querySelectorAll(".jarea__match")].filter(vis).map((e) => [e.style.getPropertyValue("--r"), e.style.getPropertyValue("--c")]);
        const g = q(".jarea__guide");
        return { band: vis(q(".jarea__band")) ? q(".jarea__band").style.getPropertyValue("--r") : null, num: q(".jarea__active-num").textContent, numShown: vis(q(".jarea__active-num")), matches,
          guide: vis(g) ? [g.style.getPropertyValue("--r"), g.style.getPropertyValue("--c"), g.style.getPropertyValue("--n")] : null }; })()`);
      if (tags < 0) {
        t.assert(false, "the seed has a member tags with an open bracket at the end of its line");
        return;
      }
      // 1. A click on a line puts the band and the strong number on it. The band has the colour of the token.
      await clickAt(p, 1, 6);
      let s = await state();
      t.eq([s.band, s.num], ["1", "2"], "the band and the number are on the clicked line");
      const bandColour = await p.eval(`[getComputedStyle(document.querySelector(".jarea__band")).backgroundColor, ${TOKEN("--json-line-active")}, getComputedStyle(document.querySelector(".jarea__active-num")).color, ${TOKEN("--json-lineno-active")}]`);
      t.eq(bandColour[0], bandColour[1], "the band has the colour of --json-line-active");
      t.eq(bandColour[2], bandColour[3], "the number of the caret line has the colour of --json-lineno-active");
      // 2. A caret right after the [ of tags: the box is on the [ and on its ], and the guide runs along the lines between them.
      const open = lines[tags].lastIndexOf("[");
      await clickAt(p, tags, open + 1);
      s = await state();
      const close = lines.findIndex((l, i) => i > tags && /^\s*\],?$/.test(l));
      t.eq(s.matches.map((m) => m[0]).sort(), [String(tags), String(close)].sort(), "the box is on the bracket and on its pair");
      t.assert(s.guide !== null && s.guide[0] === String(tags + 1) && s.guide[2] === String(close - tags - 1), `the guide runs along the lines between the brackets: ${JSON.stringify(s.guide)}`);
      t.eq(Number(s.guide[1]), lines[tags].length - lines[tags].trimStart().length, "the guide is at the indent of the open line");
      // 3. A caret in a name: no box. A selection of text: no band, no box.
      await clickAt(p, 1, 6);
      s = await state();
      t.eq(s.matches, [], "no box when the caret touches no bracket");
      await p.eval(`(() => { const a = document.querySelector('[data-testid="json-text"]'); a.setSelectionRange(2, 30); a.dispatchEvent(new Event("select")); return true; })()`);
      await sleep(250);
      s = await state();
      t.eq([s.band, s.matches], [null, []], "a selection has no band and no box");
      // 4. The guides of the indent: a line with 6 spaces has a box of 6 columns with the guide image, and the line is drawn with the same text.
      const deep = lines.findIndex((l) => /^ {6}\S/.test(l));
      const guide = await p.eval(`(() => { const e = [...document.querySelectorAll(".jarea__code .jt-ind")].find((s) => s.textContent === "      ");
        return e ? { image: getComputedStyle(e).backgroundImage.includes("repeating-linear-gradient"), width: e.getBoundingClientRect().width, ch: document.querySelector(".jarea__code span").getBoundingClientRect().width, height: e.getBoundingClientRect().height } : null; })()`);
      t.assert(deep > 0 && guide !== null, "the layer has a guide box for a line with 6 spaces");
      t.assert(guide.image, "the box draws guides with a gradient");
      t.assert(Math.abs(guide.width - 6 * guide.ch) < 1.5, `the box is 6 columns wide: ${guide.width} for ${guide.ch}`);
      t.eq(guide.height, 20, "the box has the height of a row, so the guide has no gap between rows");
      // 5. A syntax error has a wavy line under its token, in the colour of the token, and the line number has the red band.
      const broken = [...lines];
      broken[tags] = `${broken[tags]} oops`;
      await p.fill(p.tid("json-text"), broken.join("\n"));
      // The insert of the text moves the caret to the end of the text, and the area scrolls there: the layer draws the rows near the visible rows only.
      await sleep(400);
      await p.eval(`(() => { const a = document.querySelector('[data-testid="json-text"]'); a.scrollTop = 0; a.dispatchEvent(new Event("scroll")); return true; })()`);
      await p.waitFor(`!!document.querySelector(".jarea__code .jt-err")`, { what: "the wavy line" });
      const err = await p.eval(`(() => { const e = document.querySelector(".jarea__code .jt-err"); const c = getComputedStyle(e); return { text: e.textContent, line: c.textDecorationLine, style: c.textDecorationStyle, colour: c.textDecorationColor, token: ${TOKEN("--json-squiggle")} }; })()`);
      t.eq([err.text, err.line, err.style], ["oops", "underline", "wavy"], "the faulty word has a wavy underline");
      t.eq(err.colour, err.token, "the wavy line has the colour of --json-squiggle");
      t.assert(await p.eval(`!!document.querySelector(".jarea__mark--error")`), "the number of the line has the red band");
      // The text of the layer is the text of the area: the wavy line adds no character.
      t.eq(await p.eval(`document.querySelector(".jarea__code").textContent.split("\\n")[${tags}]`), broken[tags], "the layer draws the same line");
      // 6. The text is valid again: the wavy line goes.
      await p.fill(p.tid("json-text"), lines.join("\n"));
      await sleep(400);
      await p.eval(`(() => { const a = document.querySelector('[data-testid="json-text"]'); a.scrollTop = 0; a.dispatchEvent(new Event("scroll")); return true; })()`);
      await p.waitFor(`!document.querySelector(".jarea__code .jt-err")`, { what: "no wavy line" });
    },
  },
];
