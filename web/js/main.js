// Entry of the page: boot, session, router (9.5, 9.6).

import * as api from "./api.js";
import * as busy from "./busy.js";
import { app, assetUrl, basePath, boot, relativePath } from "./config.js";
import { GUARD_KEY, entryTarget, guardValue, parseGuard } from "./entry.js";
import { themeControl } from "./theme.js";
import { SPRITE_PREFIX, clear, el, icon, notFoundIllustration } from "./ui.js";

const main = document.getElementById("app-main");
const headerEl = document.getElementById("app-header");

let current = null; // { kind, uid, destroy(), locationChanged() }
let routeToken = 0;
let ahead = null; // { kind, uid, read }: the reads of the route that start before the session answer; the first view takes them
let shownOnce = false; // true after the first route of this page load started (the entry route, 9.7)
let headerMeasure = null; // the synchronous measure of the header layout (header_layout.js)
let revealed = false; // true when the first paint is released (reveal)
const shownQueue = []; // functions that wait for the release: a hidden element takes no focus

// --- header ---------------------------------------------------------------------------------------

// The product name with the version line beside it (9.6). The line is `version:checksum`, fixed at build time and read from the boot
// block of the shell, so every page shows it, also the pages of an error. It shows next to the name above 1280 px and is for assistive
// technology below. The link has the accessible name of the product; the version line is its description. The title of the link has the
// full value of the version.
const versionLine = el("span", { id: "brand-version", class: "brand__version", dataset: { testid: "app-version" }, text: boot.version, title: boot.versionTitle || null, hidden: !boot.version });
const brand = el("a", { class: "brand", href: ".", title: boot.versionTitle || null, "aria-label": "Hune, the list of dashboards", "aria-describedby": "brand-version" },
  icon("logo", 20), el("span", { class: "brand__text" }, el("span", { class: "brand__name", text: "Hune" }), versionLine));
// The place of the page (9.6): on a dashboard, the selector of the dashboards (switcher.js). It is the one flexible part of the header.
const placeNav = el("nav", { class: "location", "aria-label": "Location" });
// The controls that follow the brand at the left (the sort and the tag filter of the list): the page puts them here with header.setStart().
const startSlot = el("div", { class: "header-start", dataset: { testid: "header-start" } });
// The controls of the page (9.6): the page puts its groups in this row with header.setActions().
const actionsRow = el("div", { class: "header-actions__row", dataset: { testid: "header-actions" } });
const actionsSlot = el("div", { class: "header-actions" }, actionsRow);
const themeSlot = el("div", { class: "header-theme" });
// The button "More actions" of a dashboard (9.9): it follows the theme selector. The list page and the pages of an error have none.
const moreSlot = el("div", { class: "header-more" });

// The header holds the brand, the start controls, the place of the page, the controls of the page, the theme selector and the button "More actions", on every page.
// It is one row at every width (9.6): the css steps of header_layout.js make the controls smaller, and the narrowest steps move
// controls into the menu.
function buildHeader() {
  clear(headerEl);
  headerEl.append(brand, startSlot, placeNav, actionsSlot, themeSlot, moreSlot);
  themeSlot.append(themeControl());
  headerMeasure = trackHeaderHeight();
}

// The header is always on top and the content scrolls under it (9.6): --header-h is the real height of the header. --scrollbar-w is the width of the
// scrollbar of the content, for the parts that are fixed to the window edge (the options pane of the panel editor): 0 when the content does not scroll.
function trackHeaderHeight() {
  const root = document.documentElement;
  const measure = () => {
    root.style.setProperty("--header-h", `${headerEl.offsetHeight}px`);
    root.style.setProperty("--scrollbar-w", `${Math.max(0, main.offsetWidth - main.clientWidth)}px`);
  };
  measure();
  if (typeof ResizeObserver === "function") {
    const watch = new ResizeObserver(measure);
    watch.observe(headerEl);
    watch.observe(main);
  }
  window.addEventListener("resize", measure);
  return measure;
}

// The scroll position of the content, for each path. A page starts at the top. A Back or a Forward puts the content where it was, as the
// browser does for a document that scrolls (the content scrolls, not the document, so the browser does not restore it).
const scrollMemory = new Map();
let shownPath = null;
let restoreScroll = false;

// The API that pages use to fill the header.
const header = {
  // The nodes of the place of the page: the selector of the dashboards, and on a view of a panel the way back and the name of the view.
  setLocation(...nodes) {
    clear(placeNav);
    for (const node of nodes) if (node) placeNav.append(node);
  },
  // The controls that follow the brand, before the place of the page. A page that has none leaves the slot empty.
  setStart(...nodes) {
    clear(startSlot);
    for (const node of nodes) if (node) startSlot.append(node);
  },
  // The groups of the controls of the page. The node `actions` is the row itself, for a class that depends on the state of the page.
  setActions(...nodes) {
    clear(actionsRow);
    actionsRow.className = "header-actions__row";
    for (const node of nodes) if (node) actionsRow.append(node);
  },
  actions: actionsRow,
  // The button "More actions": it is the last control of the header, after the theme selector. A null node empties the slot.
  setMore(node) {
    clear(moreSlot);
    if (node) moreSlot.append(node);
  },
  clear() {
    clear(placeNav);
    clear(startSlot);
    clear(actionsRow);
    clear(moreSlot);
    actionsRow.className = "header-actions__row";
  },
};

// --- the first paint (9.6) -------------------------------------------------------------------------------

// The shell holds its first paint (attribute data-hune-hold, base.css). The page releases it when the first view is complete: the
// header with its controls, the page, the icons and the font. A reload then goes from the old page to the new page in one step.
// boot_theme.js releases it after 2 s in any case.
function reveal() {
  if (revealed) return;
  revealed = true;
  if (headerMeasure) headerMeasure();
  document.documentElement.removeAttribute("data-hune-hold");
  for (const run of shownQueue.splice(0)) run();
}

// Runs `run` when the first paint is released, or at once after that. A page uses it for a focus: a hidden element cannot take it.
function whenShown(run) {
  if (revealed) run();
  else shownQueue.push(run);
}

// The symbols of the sprite icons.svg go in the page as one hidden svg, with the ids prefixed (ui.js icon()). The request starts
// when this module loads. It never throws: without the sprite the icons stay empty and the page works.
async function loadSprite() {
  try {
    const answer = await fetch(assetUrl("icons.svg"), { credentials: "same-origin" });
    if (!answer.ok) return;
    const doc = new DOMParser().parseFromString(await answer.text(), "image/svg+xml");
    if (doc.documentElement.nodeName !== "svg") return;
    const sprite = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    sprite.setAttribute("aria-hidden", "true");
    sprite.setAttribute("focusable", "false");
    sprite.setAttribute("width", "0");
    sprite.setAttribute("height", "0");
    sprite.setAttribute("id", "hune-sprite");
    sprite.style.position = "absolute";
    for (const symbol of doc.querySelectorAll("symbol")) {
      const node = document.importNode(symbol, true);
      node.setAttribute("id", SPRITE_PREFIX + symbol.getAttribute("id"));
      sprite.append(node);
    }
    document.body.prepend(sprite);
  } catch (error) {
    console.error(error);
  }
}
const spriteReady = loadSprite();

// --- full pages -----------------------------------------------------------------------------------

// A full page: an icon, a title or a text, and the actions. `icon` is the name of an icon of the sprite.
export function fullPage({ title = null, text, hint = null, icon: iconName = "info", tone = null, buttons = [], links = [], testid = "full-page" }) {
  clear(main);
  main.classList.add("app-main--full"); // the card is centred in the height of the window
  // The pages "not found" (the compass icon) show a picture; the other pages show their icon in a round badge.
  const picture = iconName === "compass";
  const box = el("section", { class: `full-page${tone ? ` full-page--${tone}` : ""}${picture ? " full-page--picture" : ""}`, dataset: { testid } });
  if (picture) box.append(notFoundIllustration());
  else if (iconName) box.append(el("div", { class: "full-page__icon" }, icon(iconName, 22)));
  if (title) box.append(el("h1", { text: title }));
  box.append(el("p", { class: `full-page__text${title ? "" : " full-page__text--lead"}`, text }));
  if (hint) box.append(el("p", { class: "full-page__hint", text: hint }));
  const row = el("div", { class: "full-page__actions" });
  buttons.forEach((b, index) => row.append(el("button", { type: "button", class: `btn${index === 0 ? " btn--primary" : ""}`, text: b.label, on: { click: b.onClick } })));
  links.forEach((l, index) => row.append(el("a", { class: `btn${buttons.length === 0 && index === 0 ? " btn--primary" : ""}`, href: l.href, text: l.label })));
  if (row.childNodes.length) box.append(row);
  main.append(box);
  return box;
}

function unauthorizedPage() {
  stopCurrent();
  header.clear();
  fullPage({
    icon: "lock",
    tone: "warning",
    text: "You are not signed in, or your session ended. Reload the page.",
    hint: "Hune has no sign-in page: your sign-in service does the login.",
    buttons: [{ label: "Reload", onClick: () => window.location.reload() }],
    testid: "unauthorized",
  });
  reveal();
}

function noAnswerPage(retry) {
  stopCurrent();
  fullPage({ icon: "plug-off", tone: "error", text: "Hune does not answer.", buttons: [{ label: "Retry", onClick: retry }], testid: "no-answer" });
  reveal();
}

function notFoundPage() {
  document.documentElement.setAttribute("data-hune-view", "not-found");
  header.clear();
  document.title = "Page not found - Hune";
  fullPage({ icon: "compass", title: "Page not found", text: "This address does not exist.", links: [{ label: "Go to the list", href: "." }], testid: "not-found" });
}

// --- router ---------------------------------------------------------------------------------------

function stopCurrent() {
  if (current && current.destroy) current.destroy();
  current = null;
}

function parseRoute() {
  const rel = relativePath();
  if (rel === "") return { kind: "list" };
  const match = /^d\/([a-z0-9][a-z0-9-]{0,39})(?:\/([^/]+))?\/?$/.exec(rel);
  if (match) return { kind: "dashboard", uid: match[1], slug: match[2] || null };
  return { kind: "none" };
}

const ctx = {
  main,
  header,
  fullPage,
  navigate,
  whenShown,
  get session() {
    return app.session;
  },
};

// The guard against a second entry redirect to the same uid in this tab (9.7). Storage can fail: no guard then.
function readGuard() {
  try {
    return parseGuard(window.sessionStorage.getItem(GUARD_KEY));
  } catch (error) {
    return null;
  }
}

function writeGuard(uid) {
  try {
    window.sessionStorage.setItem(GUARD_KEY, guardValue(uid, Date.now()));
  } catch (error) {
    // The redirect still happens: it replaces the history entry, so it cannot loop by itself.
  }
}

// The hook of the list page (9.7): it runs after the first full load of the list. It returns true when it
// navigates away, so the list does not render. Only the entry route of the page load can redirect.
function entryHook(isEntry) {
  return ({ items }) => {
    const target = entryTarget({ items, search: new URLSearchParams(window.location.search).get("q") || "", isEntry, guard: readGuard(), now: Date.now() });
    if (target === null) return false;
    writeGuard(items[0].uid);
    navigate(target, { replace: true });
    return true;
  };
}

// `entry` is set by a retry of a failed entry route: it stays the entry route. Else the first call is the entry route.
async function show({ entry = null } = {}) {
  const token = ++routeToken;
  const wantsRestore = restoreScroll;
  restoreScroll = false;
  const isEntry = entry === null ? !shownOnce : entry;
  shownOnce = true;
  const route = parseRoute();
  if (current && route.kind === "dashboard" && current.kind === "dashboard" && current.uid === route.uid && current.locationChanged) {
    current.locationChanged();
    return;
  }
  if (shownPath !== null) scrollMemory.set(shownPath, main.scrollTop);
  shownPath = window.location.pathname;
  const reads = takeAhead(route);
  stopCurrent();
  // The hook of the state of the page (9.29) is absent while a view loads and on the pages of an error.
  document.documentElement.removeAttribute("data-hune-view");
  document.documentElement.removeAttribute("data-hune-origin");
  header.clear();
  clear(main);
  main.classList.remove("app-main--full");
  document.title = "Hune";
  busy.begin();
  try {
    let page = null;
    if (route.kind === "list") {
      const mod = await import("./list.js");
      page = await mod.mount({ ...ctx, afterLoad: entryHook(isEntry), ahead: reads });
      if (page && token === routeToken && !main.classList.contains("app-main--full")) document.documentElement.setAttribute("data-hune-view", "list");
    } else if (route.kind === "dashboard") {
      const mod = await import("./dashboard.js");
      page = await mod.mount({ ...ctx, uid: route.uid, slug: route.slug, ahead: reads });
    } else {
      notFoundPage();
    }
    if (token !== routeToken) {
      if (page && page.destroy) page.destroy();
      return;
    }
    current = page ? { kind: route.kind, uid: route.uid, ...page } : null;
  } catch (error) {
    if (reads) reads.abort(); // The module did not load: nobody takes the reads.
    if (token === routeToken && !api.isAbort(error)) {
      if (error && error.name === "ApiError" && error.status === 401) return; // The 401 hook shows the page.
      console.error(error);
      noAnswerPage(() => show({ entry: isEntry }));
    }
  } finally {
    busy.end();
    if (token === routeToken) main.scrollTop = wantsRestore ? scrollMemory.get(window.location.pathname) || 0 : 0;
    // A route that another route replaced (the entry redirect of 9.7) does not release the paint: the last route does.
    if (token === routeToken) reveal();
  }
}

// Changes the view with history.pushState and no reload (9.5). `url` is a URL object or a text.
export function navigate(url, { replace = false } = {}) {
  const target = new URL(url, document.baseURI);
  if (replace) window.history.replaceState(null, "", target);
  else window.history.pushState(null, "", target);
  return show();
}

// The skip link of the shell (shell.html) goes to <main>. The shell has a <base>, so the browser would resolve "#app-main" against
// the base path and open the list. This handler moves the focus to <main> in the page instead. <main> is focusable only for this move.
function skipToMain(event) {
  event.preventDefault();
  main.setAttribute("tabindex", "-1");
  main.addEventListener("blur", () => main.removeAttribute("tabindex"), { once: true });
  main.focus();
}

function onClick(event) {
  if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  const link = event.target instanceof Element ? event.target.closest("a[href]") : null;
  if (!link || link.target || link.hasAttribute("download")) return;
  const url = new URL(link.href, document.baseURI);
  const base = basePath();
  if (url.origin !== window.location.origin || !url.pathname.startsWith(base)) return;
  const rel = url.pathname.slice(base.length);
  if (rel.startsWith("api/") || rel.startsWith("static/") || rel.startsWith(".well-known/")) return;
  if (link.dataset.external === "true") return;
  event.preventDefault();
  navigate(url);
}

// --- start ----------------------------------------------------------------------------------------

let started = false;

// Waits for the font Inter (weights 400 and 500) before the first render, so the canvas text and the text measures use it.
// It waits one second at most, it never throws and it never blocks longer: a font that does not load changes nothing else.
function fontsReady() {
  try {
    const fonts = document.fonts;
    if (!fonts || typeof fonts.load !== "function") return Promise.resolve();
    let timer = 0;
    const loaded = Promise.all([fonts.load("400 14px Inter"), fonts.load("500 14px Inter")]).catch(() => {});
    const late = new Promise((resolve) => {
      timer = setTimeout(resolve, FONT_WAIT_MS);
    });
    return Promise.race([loaded, late]).then(() => clearTimeout(timer), () => clearTimeout(timer));
  } catch (error) {
    return Promise.resolve();
  }
}
const FONT_WAIT_MS = 1000;

// The module of the page of the route loads while the session request is in flight, not after it. The reads of the page start then too.
function preloadRoute() {
  dropAhead();
  const route = parseRoute();
  const load = route.kind === "dashboard" ? import("./dashboard.js") : route.kind === "list" ? import("./list.js") : null;
  if (load) load.catch(() => {});
  if (route.kind === "dashboard") ahead = { kind: "dashboard", uid: route.uid, read: api.startAhead((signal) => api.dashboard(route.uid, signal)) };
  else if (route.kind === "list") ahead = { kind: "list", uid: undefined, read: api.startAhead((signal) => Promise.all([api.folders(signal), api.dashboards(signal)])) };
}

function dropAhead() {
  if (ahead) ahead.read.abort();
  ahead = null;
}

// The first view takes the reads, if its route and its uid are the same. Else they end, and the page sends its own.
function takeAhead(route) {
  const held = ahead;
  ahead = null;
  if (!held) return null;
  if (held.kind === route.kind && held.uid === route.uid) return held.read;
  held.read.abort();
  return null;
}

async function start() {
  const fontWait = fontsReady();
  preloadRoute();
  if (!started) {
    started = true;
    buildHeader();
    api.setUnauthorizedHandler(unauthorizedPage);
    const skip = document.getElementById("skip-link");
    if (skip) skip.addEventListener("click", skipToMain);
    document.addEventListener("click", onClick);
    window.addEventListener("popstate", () => {
      restoreScroll = true;
      show();
    });
  }
  try {
    app.session = await api.session();
  } catch (error) {
    dropAhead();
    if (error && error.status === 401) return; // The hook shows the page of step 3.
    noAnswerPage(() => {
      busy.begin();
      start().finally(busy.end);
    });
    return;
  }
  if (app.session.build && boot.build && app.session.build !== boot.build) api.showBuildNotice();
  await Promise.all([fontWait, spriteReady]);
  await show();
}

busy.begin();
start()
  .catch((error) => console.error(error))
  .finally(() => {
    busy.end();
    reveal();
  });
