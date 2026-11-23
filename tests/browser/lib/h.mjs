// Small helpers shared by the cases.

/** `{ "1": "ready", ... }` from the `data-state` of each panel frame. */
export async function panelStates(page) {
  return page.eval(`(() => {
    const out = {};
    for (const e of document.querySelectorAll('[data-testid^="panel-"][data-state]')) {
      const m = /^panel-(\\d+)$/.exec(e.dataset.testid);
      if (m) out[m[1]] = e.dataset.state;
    }
    return out;
  })()`);
}

export async function waitPanelsReady(page, ids, state = "ready", root = "") {
  await page.waitFor(
    `(() => ${JSON.stringify(ids)}.every((id) => { const e = document.querySelector(${JSON.stringify(root)} + ' [data-testid="panel-' + id + '"]'); return e && e.dataset.state === ${JSON.stringify(state)}; }))()`,
    { what: `panels ${ids} ${state}` },
  );
  await page.idle();
}

/** Open "View data" of a panel and return `{ headers, rows }` as texts. */
export async function viewData(page, id) {
  await page.click(page.tid(`panel-${id}-menu`));
  await page.click(page.tid(`panel-${id}-view-data`));
  await page.waitSel(page.tid(`view-data-${id}`));
  const data = await page.eval(`(() => {
    const d = document.querySelector('[data-testid="view-data-${id}"]');
    const headers = [...d.querySelectorAll("thead th")].map((e) => e.textContent);
    const rows = [...d.querySelectorAll("tbody tr")].map((r) => [...r.children].map((c) => c.textContent));
    return { headers, rows };
  })()`);
  await page.press("Escape");
  await page.waitGone(page.tid(`view-data-${id}`)).catch(() => {});
  return data;
}

/** Query requests (POST .../panels/<id>/query) seen by a page. */
export function panelQueries(page) {
  return page.requests.filter((r) => r.method === "POST" && /\/panels\/\d+\/query$/.test(r.url));
}

export function optionQueries(page) {
  return page.requests.filter((r) => r.method === "POST" && /\/variables\/[^/]+\/options$/.test(r.url));
}

export const panelIdOf = (r) => Number(/\/panels\/(\d+)\/query$/.exec(r.url)[1]);

export function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

/** Read a history directory of a volume: the sorted list of file names. */
export async function historyFiles(server, uid) {
  const { readdirSync } = await import("node:fs");
  const { join } = await import("node:path");
  try {
    return readdirSync(join(server.volume, ".history", uid)).sort();
  } catch {
    return [];
  }
}

/** Scroll each panel into view and wait until it is in the state (a panel far from the viewport does not load). */
export async function reveal(page, ids, state = "ready") {
  for (const id of ids) {
    await page.eval(`document.querySelector('[data-testid="panel-${id}"]').scrollIntoView({block: "center"})`);
    await page.waitFor(`(document.querySelector('[data-testid="panel-${id}"]') || {dataset: {}}).dataset.state === ${JSON.stringify(state)}`, { what: `panel ${id} ${state}` });
  }
  await page.idle();
}

/** The texts drawn on the chart canvas of a panel, in draw order (needs `page.recordCanvasText()`): `{ x: [..], y: [..] }`. */
export async function axisTexts(page, id) {
  const all = await page.eval(`(() => {
    const c = document.querySelector('[data-testid="panel-${id}"] canvas.chartCore__canvas');
    return c ? window.__canvasTexts(c) : null;
  })()`);
  if (!all) return null;
  return { y: all.filter((t) => t.align === "right").map((t) => t.text), x: all.filter((t) => t.align === "center" && t.baseline === "top").map((t) => t.text), all };
}

/** The number of opaque pixels of the chart canvas of a panel that have this colour (each channel within `tol`). */
export async function colourPixels(page, id, [r, g, b], tol = 12) {
  return page.eval(`(() => {
    const c = document.querySelector('[data-testid="panel-${id}"] canvas.chartCore__canvas');
    const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
    let n = 0;
    for (let i = 0; i < d.length; i += 4) {
      if (d[i + 3] > 200 && Math.abs(d[i] - ${r}) <= ${tol} && Math.abs(d[i + 1] - ${g}) <= ${tol} && Math.abs(d[i + 2] - ${b}) <= ${tol}) n++;
    }
    return n;
  })()`);
}

/** The tooltip of a chart after a hover at the fraction `fx` of the plot: `{ head, rows: [{label, value}] }`, or null when it is hidden. */
export async function tooltipAt(page, id, fx = 0.5) {
  await page.hover(`[data-testid="panel-${id}"] .chartCore__overlay`, fx, 0.5);
  await page.waitFor(`(() => { const t = document.querySelector('[data-testid="panel-${id}"] .chartCore__tooltip'); return t && !t.hidden; })()`, { what: "tooltip", timeout: 3000 }).catch(() => {});
  return page.eval(`(() => {
    const t = document.querySelector('[data-testid="panel-${id}"] .chartCore__tooltip');
    if (!t || t.hidden) return null;
    return { head: t.querySelector("strong") ? t.querySelector("strong").textContent : "", rows: [...t.querySelectorAll(".chartCore__tipRow")].map((r) => ({ label: r.querySelector("em").textContent, value: r.querySelector("b").textContent })) };
  })()`);
}

/** The data of the threshold hook (`data-thresholds` of the plot element, 9.29) of a panel, or null. */
export async function thresholdSnapshot(page, id) {
  const text = await page.eval(`(() => { const e = document.querySelector('[data-testid="panel-${id}"] .panel__plot'); return e ? e.getAttribute("data-thresholds") : null; })()`);
  return text ? JSON.parse(text) : null;
}
