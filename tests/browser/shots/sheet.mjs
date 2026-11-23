// SPDX-License-Identifier: MIT OR Apache-2.0
// The contact sheet of a run: one index.html with relative image links only. It needs no server.
import { writeFileSync } from "node:fs";
import { join } from "node:path";

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

export function writeIndex(outDir, manifest, states) {
  const byState = new Map();
  for (const s of manifest.shots) {
    if (!byState.has(s.state)) byState.set(s.state, []);
    byState.get(s.state).push(s);
  }
  const text = new Map(states.map((s) => [s.id, s.text]));
  const order = states.map((s) => s.id).filter((id) => byState.has(id));
  const themeRank = { light: 0, dark: 1, "auto-dark": 2, "auto-light": 3 };
  const sections = order
    .map((id) => {
      const shots = byState.get(id).sort((a, b) => a.suffix.localeCompare(b.suffix) || themeRank[a.theme] - themeRank[b.theme] || b.width - a.width);
      const figs = shots
        .map((s) => {
          const bad = s.problems && s.problems.length ? ` <span class="bad" title="${esc(s.problems.join("\n"))}">console</span>` : "";
          return `<figure data-theme="${s.theme}" data-width="${s.width}"><a href="${esc(s.file)}"><img loading="lazy" src="${esc(s.file)}" alt="${esc(id)} ${s.theme} ${s.width}"></a><figcaption>${s.suffix ? esc(s.suffix) + " · " : ""}${s.theme} · ${s.width}${bad}</figcaption></figure>`;
        })
        .join("\n");
      return `<section id="${esc(id)}" data-state="${esc(id)}"><h2>${esc(id)} <small>${esc(text.get(id) || "")}</small></h2><div class="grid">${figs}</div></section>`;
    })
    .join("\n");
  const fails = manifest.failures.length
    ? `<section class="fails"><h2>Failures (${manifest.failures.length})</h2><ul>${manifest.failures.map((f) => `<li><code>${esc(f.state)} ${f.theme} ${f.width}</code> ${esc(f.error.split("\n")[0])}</li>`).join("")}</ul></section>`
    : "";
  const nav = order.map((id) => `<a href="#${esc(id)}">${esc(id)}</a>`).join(" ");
  const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Hune screenshots: ${esc(manifest.label)}</title>
<style>
body{font:14px/1.4 system-ui,sans-serif;margin:0;background:#eef0f3;color:#1b1f24}
header{position:sticky;top:0;background:#fff;border-bottom:1px solid #c9ced6;padding:8px 16px;z-index:2}
header label{margin-right:12px}
nav{font-size:12px;max-height:3.2em;overflow:auto;margin-top:6px}nav a{margin-right:8px;white-space:nowrap}
main{padding:8px 16px}
section{margin:18px 0}h2{font-size:16px;margin:0 0 6px}h2 small{font-weight:400;color:#5b6370;margin-left:8px}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(250px,1fr));gap:10px}
figure{margin:0;background:#fff;border:1px solid #c9ced6;padding:6px}
figure[hidden]{display:none}
img{display:block;width:100%;height:200px;object-fit:cover;object-position:top left;background:#ddd}
figcaption{font-size:12px;color:#444;margin-top:4px}
.bad{background:#b3261e;color:#fff;border-radius:3px;padding:0 4px;margin-left:4px}
.fails{background:#fff3f2;border:1px solid #b3261e;padding:8px 12px}
</style></head><body>
<header><strong>Hune screenshots: ${esc(manifest.label)}</strong>
 <span>${manifest.shots.length} screenshots, ${manifest.failures.length} failures, ${manifest.seconds || 0} s</span><br>
 <label>Theme <select id="ft"><option value="">all</option><option>light</option><option>dark</option><option>auto-dark</option><option>auto-light</option></select></label>
 <label>Width <select id="fw"><option value="">all</option><option>1920</option><option>1280</option><option>768</option><option>390</option></select></label>
 <label>State <input id="fs" type="search" placeholder="filter"></label>
 <nav>${nav}</nav></header>
<main>${fails}${sections}</main>
<script>
(function () {
  var ft = document.getElementById("ft"), fw = document.getElementById("fw"), fs = document.getElementById("fs");
  function apply() {
    var t = ft.value, w = fw.value, s = fs.value.toLowerCase();
    document.querySelectorAll("section[data-state]").forEach(function (sec) {
      var shown = 0;
      sec.querySelectorAll("figure").forEach(function (f) {
        var ok = (!t || f.dataset.theme === t) && (!w || f.dataset.width === w);
        f.hidden = !ok;
        if (ok) shown++;
      });
      sec.hidden = !shown || (s && sec.dataset.state.toLowerCase().indexOf(s) < 0);
    });
  }
  ft.onchange = fw.onchange = apply;
  fs.oninput = apply;
})();
</script></body></html>
`;
  writeFileSync(join(outDir, "index.html"), html);
}
