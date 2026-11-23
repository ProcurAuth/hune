// SPDX-License-Identifier: MIT OR Apache-2.0
// Page helpers of the screenshot harness: emulation, settle, capture, hold of requests.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Viewport height for each width: a desktop, a wide desktop, a tablet and a phone. */
export const HEIGHTS = { 1280: 900, 1920: 1080, 768: 1024, 390: 844 };

/** The themes of the harness: the stored choice and the emulated system preference. */
export const THEMES = {
  light: { choice: "light", scheme: "light" },
  dark: { choice: "dark", scheme: "dark" },
  "auto-dark": { choice: "system", scheme: "dark" },
  "auto-light": { choice: "system", scheme: "light" },
};

/** Open a page of a context with the emulation of a theme and a width. */
export async function openPage(context, { theme, width }) {
  const height = HEIGHTS[width] || 900;
  const t = THEMES[theme];
  const page = await context.newPage({ width, height });
  page.base = { width, height };
  await page.send("Emulation.setEmulatedMedia", {
    features: [
      { name: "prefers-reduced-motion", value: "reduce" },
      { name: "prefers-color-scheme", value: t.scheme },
    ],
  });
  // The choice of the theme is set before the first script of each document.
  await page.send("Page.addScriptToEvaluateOnNewDocument", {
    source: `try { localStorage.setItem("hune.theme", ${JSON.stringify(t.choice)}); } catch (e) {}`,
  });
  // A request interceptor of this module: see hold().
  const orig = page._event.bind(page);
  page._event = (method, params) => {
    if (method === "Fetch.requestPaused" && page.interceptor && page.interceptor(params)) return;
    orig(method, params);
  };
  return page;
}

/** True when the panels that are in the layout have an answer (ready, empty or error). */
const PANELS_SETTLED = `(() => {
  for (const e of document.querySelectorAll('[data-testid^="panel-"][data-state]')) {
    if (!/^panel-\\d+$/.test(e.dataset.testid)) continue;
    if (e.offsetParent === null) continue;
    if (!["ready", "empty", "error"].includes(e.dataset.state)) return false;
  }
  return true;
})()`;

/** Wait for the idle hook, for the panels, for the fonts, then for two frames. */
export async function settle(page, { timeout = 25000, panels = true } = {}) {
  await page.idle({ timeout });
  if (panels) await page.waitFor(PANELS_SETTLED, { timeout, what: "panels settled" }).catch(() => {});
  await page.eval(`document.fonts ? document.fonts.ready.then(() => true) : true`).catch(() => {});
  await page.eval(`new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r(true))))`).catch(() => {});
  await sleep(120);
}

async function setMetrics(page, width, height) {
  await page.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
}

const DOC_HEIGHT = `Math.max(document.documentElement.scrollHeight, document.body ? document.body.scrollHeight : 0)`;

function save(path, data) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, Buffer.from(data, "base64"));
}

/**
 * Capture a screenshot.
 *   full: the whole page (the viewport grows to the document height, up to maxHeight).
 *   selector or rect: a clip of the document (in CSS pixels). `pad` adds space around the element.
 * With none of them, the screenshot is the viewport.
 */
export async function capture(page, path, { full = false, selector = null, selectors = null, pad = 0, maxHeight = 9000, noSettle = false, noGrow = false } = {}) {
  const { width, height } = page.base;
  // Pages that are not Hune pages have no idle hook: wait a fixed time.
  const wait = () => (noSettle ? sleep(400) : settle(page));
  if (selector || selectors) {
    const list = selectors || [selector];
    // Make room: a tall element needs a tall viewport.
    const rect = await clipOf(page, list, pad);
    if (!rect) throw new Error(`no element for ${list.join(", ")}`);
    if (noGrow) {
      // No resize and no wait: the page keeps its state (a panel that is still loading, for example).
      const r0 = await page.send("Page.captureScreenshot", { format: "png", clip: { ...rect, scale: 1 }, captureBeyondViewport: true });
      save(path, r0.data);
      return;
    }
    const need = Math.min(maxHeight, Math.ceil(rect.y + rect.height + 20));
    let grown = false;
    if (need > height) {
      await setMetrics(page, width, need);
      grown = true;
      await wait();
    }
    const r2 = await clipOf(page, list, pad);
    const clip = r2 || rect;
    const r = await page.send("Page.captureScreenshot", { format: "png", clip: { ...clip, scale: 1 }, captureBeyondViewport: false });
    if (grown) await setMetrics(page, width, height);
    save(path, r.data);
    return;
  }
  if (full) {
    let h = await page.eval(DOC_HEIGHT);
    let applied = height;
    for (let i = 0; i < 3 && h > applied && applied < maxHeight; i++) {
      applied = Math.min(h, maxHeight);
      await setMetrics(page, width, applied);
      await wait();
      h = await page.eval(DOC_HEIGHT);
    }
    const r = await page.send("Page.captureScreenshot", { format: "png" });
    if (applied !== height) await setMetrics(page, width, height);
    save(path, r.data);
    return;
  }
  const r = await page.send("Page.captureScreenshot", { format: "png" });
  save(path, r.data);
}

/** The union of the rectangles of the elements, in document coordinates. */
async function clipOf(page, selectors, pad) {
  return page.eval(`(() => {
    let box = null;
    for (const s of ${JSON.stringify(selectors)}) {
      const e = document.querySelector(s);
      if (!e) continue;
      const b = e.getBoundingClientRect();
      if (!b.width || !b.height) continue;
      const r = { l: b.left + scrollX, t: b.top + scrollY, r: b.right + scrollX, b: b.bottom + scrollY };
      box = box ? { l: Math.min(box.l, r.l), t: Math.min(box.t, r.t), r: Math.max(box.r, r.r), b: Math.max(box.b, r.b) } : r;
    }
    if (!box) return null;
    const pad = ${pad};
    const x = Math.max(0, Math.floor(box.l - pad)), y = Math.max(0, Math.floor(box.t - pad));
    return { x, y, width: Math.ceil(box.r + pad - x), height: Math.ceil(box.b + pad - y) };
  })()`);
}

/**
 * Hold the requests that `pred(request)` accepts until release() runs. Other requests go on.
 * It replaces the catch-all pattern of the page by one pattern, so that nothing else is paused.
 */
export async function hold(page, pred) {
  const held = [];
  page.interceptor = (p) => {
    if (pred(p.request)) {
      held.push(p.requestId);
      return true;
    }
    return false;
  };
  return {
    count: () => held.length,
    async release() {
      page.interceptor = null;
      for (const id of held.splice(0)) await page.send("Fetch.continueRequest", { requestId: id }).catch(() => {});
    },
  };
}

/** Move the mouse to a point (CSS pixels of the viewport). */
export async function moveMouse(page, x, y) {
  await page.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
}

/** Move the mouse over a selector, at a fraction of its box. Scrolls it into view first. */
export async function hover(page, selector, fx = 0.5, fy = 0.5) {
  await page.waitSel(selector);
  const pt = await page.eval(`(() => {
    const e = document.querySelector(${JSON.stringify(selector)});
    e.scrollIntoView({ block: "center", inline: "nearest" });
    const b = e.getBoundingClientRect();
    return { x: b.left + b.width * ${fx}, y: b.top + b.height * ${fy} };
  })()`);
  await moveMouse(page, pt.x, pt.y - 0);
  await sleep(60);
  await moveMouse(page, pt.x + 1, pt.y + 1);
  await sleep(150);
  return pt;
}

/**
 * Move the mouse over the box of `rectSel` on a grid of points until the element `shownSel` is visible
 * (a tooltip). It tries the points in the order of `ys` then `xs`. It returns the point or null.
 */
export async function sweepHover(page, rectSel, shownSel, { xs = null, ys = null } = {}) {
  await page.waitSel(rectSel);
  await page.eval(`document.querySelector(${JSON.stringify(rectSel)}).scrollIntoView({ block: "center", inline: "nearest" })`);
  await sleep(150);
  const box = await page.eval(`(() => { const b = document.querySelector(${JSON.stringify(rectSel)}).getBoundingClientRect(); return { l: b.left, t: b.top, w: b.width, h: b.height }; })()`);
  const gx = xs || Array.from({ length: 17 }, (_, i) => 0.1 + i * 0.05);
  const gy = ys || [0.5, 0.65, 0.8, 0.35, 0.2, 0.9];
  const shown = `(() => { const e = document.querySelector(${JSON.stringify(shownSel)}); return !!e && !e.hidden && getComputedStyle(e).display !== "none" && e.getBoundingClientRect().width > 0; })()`;
  for (const fy of gy) {
    for (const fx of gx) {
      const x = box.l + box.w * fx;
      const y = box.t + box.h * fy;
      await moveMouse(page, x, y);
      await sleep(40);
      if (await page.eval(shown)) return { x, y };
    }
  }
  return null;
}
