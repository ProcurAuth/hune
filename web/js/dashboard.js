// The dashboard page (9.8): load, header, pickers, variables bar, grid, URL state, refresh, reload on 409.

import * as api from "./api.js";
import * as busy from "./busy.js";
import { loadEditorModule } from "./editor_loader.js";
import { createLinksBar, fetchItems } from "./dash_links.js";
import { createGrid } from "./grid.js";
import { createPanelFrame } from "./panel_frame.js";
import { app } from "./config.js";
import { copyKeyOf, limitsOf, repeatLimits, splitKey } from "./repeat.js";
import { gridDoc } from "./var_layout.js";
import { timeInMenu } from "./header_layout.js";
import { createTimeControl } from "./pickers.js";
import { Runtime } from "./runtime.js";
import { REFRESH_CHOICES, cut, effectiveZone, isKnownZone, refreshToMs, resolveRange, shiftRange, zoneName, zoomOut } from "./time.js";
import { buildSearch, dashboardPath, parseSearch, shortQuery } from "./url_state.js";
import { sameSelection, selectionToUrl } from "./var_options.js";
import { createVarsBar } from "./vars.js";
import { button, clear, closeMenu, dismissNotice, el, icon, notify, openMenu } from "./ui.js";
import { baseState, isEditorState, kindOf, nextState, normalSearch, originValue, parseView, readOrigin, sameState, viewNames } from "./view_state.js";
import { exportDashboard } from "./list.js";
import { locationNodes } from "./switcher.js";
import { READ_ONLY_TEXT, STALE_PAGE_TEXT, isStale } from "./stores.js";

function errorPage(ctx, error) {
  const { fullPage } = ctx;
  if (error.status === 404) {
    document.title = "Dashboard not found - Hune";
    return fullPage({ icon: "compass", title: "Dashboard not found", text: "This dashboard does not exist.", links: [{ label: "Go to the list", href: "." }], testid: "dashboard-not-found" });
  }
  if (error.status === 403) return fullPage({ icon: "lock", tone: "warning", text: "You have no access to this dashboard.", links: [{ label: "Go to the list", href: "." }], testid: "dashboard-forbidden" });
  if (error.code === "stored_document_invalid") {
    const box = fullPage({ icon: "error", tone: "error", title: "This dashboard is not valid", text: "The stored document breaks the rules of the format:", links: [{ label: "Go to the list", href: "." }], testid: "dashboard-invalid" });
    const list = el("ul", { class: "error-list error-list--box" });
    for (const e of (error.extra && error.extra.errors) || []) list.append(el("li", {}, el("code", { text: e.pointer }), ` ${e.message}`));
    box.insertBefore(list, box.querySelector(".full-page__actions"));
    return box;
  }
  return fullPage({ icon: "error", tone: "error", title: "The dashboard did not load.", text: error.message, buttons: [{ label: "Retry", onClick: () => window.location.reload() }], testid: "dashboard-error" });
}

export async function mount(ctx) {
  const { main, header, session, uid, whenShown = (run) => run() } = ctx;
  const controller = new AbortController();
  let destroyed = false;
  let rt = null;
  let grid = null;
  let bar = null;
  let linksBar = null; // the group of the dashboard links in the toolbar row (9.8a)
  let zoneNoticeFor = null; // the zone name that the last notice of an unknown zone named
  let urlZone = null; // the known zone of the URL name `timezone`, the zone of the view (9.5)
  let staleNoticed = false; // true after the notice of a file that is not valid: it shows once for the page
  let namesNoticed = false; // true after the notice of a member `names` that is absent or wrong (9.10): it shows once for the page
  let toolbar = null; // the row of the variables bar, under the header (9.9)
  let editBar = null; // the group of the edit mode (Add, Save, Revert), before the time control (10.2)
  let editSwitch = null; // the switch "Edit" of the header (10.2)
  let editActive = false; // true while the editor holds the draft: the switch is on
  let timeControl = null;
  let viewTails = { view: null, edit: null }; // true while the page shows the view of a panel or the panel editor (the header then has the way back)
  let timePicker = null;
  let refreshPicker = null;
  let viewHost = null;
  let editHost = null;
  let editHooks = null;
  let gridMarker = null;
  let viewFrame = null;
  let refresh = null;
  let refreshTimer = 0;
  let skippedTick = false;
  let loaded = null; // { doc, etag, version, folder, access, readOnly, stale }
  let state = baseState(); // the state of the page (10.2a): what the URL names
  let shown = baseState(); // the state whose view is in the page now
  let opener = null; // the hook of the control that opened the state that closes now, for the focus after a leave (10.2a, rule 14)
  const openers = new Map(); // state (page and panel) -> the hook of the control that opened it
  let focusBack = false; // true after a leave or a close of the panel view: the focus goes to the opener
  let unlisten = [];

  document.title = "Hune";
  busy.begin();
  let answer;
  try {
    answer = await api.dashboard(uid, controller.signal);
  } catch (error) {
    busy.end();
    if (api.isAbort(error)) return { destroy() {} };
    if (error.status === 401) return { destroy() {} };
    errorPage(ctx, error);
    return { destroy() {} };
  }
  busy.end();

  // --- start state --------------------------------------------------------------------------------------

  function selectionsOf(runtime) {
    return new Map(runtime.variables.map((v) => [v.name, runtime.selections.get(v.name)]));
  }

  function currentRange() {
    return rt ? { fromText: rt.range.fromText, toText: rt.range.toText } : { fromText: loaded.doc.time.from, toText: loaded.doc.time.to };
  }

  // The query part of the URL state (9.5): the range, the time zone, the variables and the refresh. A name is written only when its value differs
  // from the default of the dashboard (`shortQuery`). A variable whose default is not known yet (a query variable before its options load) stays.
  function queryState() {
    const doc = loaded.doc;
    const vars = new Map();
    const defaultVars = new Map();
    for (const v of rt.variables) {
      vars.set(v.name, selectionToUrl(v, rt.selections.get(v.name)));
      const known = rt.defaultSelection(v.name);
      if (known) defaultVars.set(v.name, selectionToUrl(v, known));
    }
    const r = currentRange();
    // A zone that is the zone of the document, or the zone that the document names resolves to, is the default zone.
    const zone = urlZone && urlZone !== doc.time.timezone && urlZone !== effectiveZone(doc.time.timezone) ? urlZone : null;
    return shortQuery(
      { from: r.fromText, to: r.toText, timezone: zone, vars, refresh: refresh === null ? "off" : refresh },
      { from: doc.time.from, to: doc.time.to, timezone: null, vars: defaultVars, refresh: doc.time.refresh ? doc.time.refresh : "off" },
    );
  }

  // The URL state of a page state: the query part and the view names of that state (10.2a).
  function urlState(target = state) {
    return { ...queryState(), ...viewNames(target) };
  }

  // The path and the query of a page state, relative to the base of the page.
  function relativeUrl(target = state) {
    return `${dashboardPath(uid, loaded.doc.title)}${buildSearch(urlState(target), rt.variables.map((v) => v.name))}`;
  }

  // The relative URL of the entry that is current now: the origin that an entry of the next state names (10.2a).
  function relativeNow() {
    const base = new URL(document.baseURI).pathname;
    const path = window.location.pathname.startsWith(base) ? window.location.pathname.slice(base.length) : window.location.pathname;
    return `${path}${window.location.search}`;
  }

  // True while the location still names this dashboard. A Back or Forward during the load moves the location to another
  // route: the page then writes no URL, so it never puts its address over the entry of another page.
  function stillHere() {
    if (destroyed) return false;
    const base = new URL(document.baseURI).pathname;
    const rel = window.location.pathname.startsWith(base) ? window.location.pathname.slice(base.length) : "";
    return rel === `d/${uid}` || rel.startsWith(`d/${uid}/`);
  }

  // The URL after a change of the range, the refresh or a variable (10.2a, rule 12): replace, in the same state. The entry keeps its origin.
  // A push (the drag on a chart, 9.8) has no origin.
  function writeUrl(push = false) {
    // The edit mode listens to this event: its button "Save" shows when the current range or refresh differs from the default (10.9).
    document.dispatchEvent(new Event("hune-query"));
    if (!stillHere()) return;
    const url = new URL(relativeUrl(), document.baseURI);
    if (push) window.history.pushState(null, "", url);
    else window.history.replaceState(window.history.state, "", url);
  }

  // The notice of a value that a variable dropped (9.10): one for each variable, in the area of the notices. The address gets the new
  // value, so a reload of it shows no second notice. The runtime calls this during its constructor: the work waits for the end of the call.
  function variableNotice({ name, text }) {
    queueMicrotask(() => {
      const variable = rt && rt.varByName.get(name);
      if (!variable || !stillHere()) return;
      const node = notify({ level: "info", key: `variable-${name}`, text });
      if (node) node.dataset.notice = "variable";
      const search = parseSearch(window.location.search);
      search.vars.set(name, selectionToUrl(variable, rt.selections.get(name)));
      window.history.replaceState(window.history.state, "", `${window.location.pathname}${buildSearch(search, rt.variables.map((v) => v.name))}`);
    });
  }

  // The URL of a state that the page opens: a push with the origin, a push with no origin, or a replace (10.2a).
  function writeEntry(target, how, withOrigin) {
    if (!stillHere()) return;
    const url = new URL(relativeUrl(target), document.baseURI);
    if (how === "push") window.history.pushState(withOrigin ? originValue(relativeNow()) : null, "", url);
    else window.history.replaceState(window.history.state, "", url);
  }

  // A view ends with a Back to the entry that it started from (10.2a, rule 7). That entry holds the range, the refresh and the variables that the
  // dashboard had before the view. A person who changed them in the view keeps them: the page keeps its own values over those of the entry, and it
  // writes them in the entry. A Back or Forward of the browser has no such carry: it shows the values of the entry that it reaches.
  let carriedQuery = null;
  function popKeepingQuery() {
    carriedQuery = { at: Date.now(), query: queryState() };
    window.history.back();
  }

  // The URL of the view of one panel. `key` is the id of the panel, or `<id>-r<n>` for a copy of a repeat (9.8c).
  function panelUrl(key) {
    return relativeUrl({ ...baseState(), page: "panel-view", panel: key });
  }

  // --- refresh ------------------------------------------------------------------------------------------------

  function setRefresh(value) {
    refresh = value;
    clearInterval(refreshTimer);
    refreshTimer = 0;
    const ms = refreshToMs(value);
    if (ms) refreshTimer = setInterval(tick, ms);
    if (refreshPicker) refreshPicker.set(value);
  }

  function tick() {
    if (document.hidden) {
      skippedTick = true;
      return;
    }
    // The page skips a tick while a request of the previous refresh is in flight or queued.
    if (!rt || rt.isBusy()) return;
    refreshAll();
  }

  function refreshAll() {
    rt.refreshNow();
    if (timePicker) timePicker.update();
  }

  function onVisibility() {
    if (!document.hidden && skippedTick) {
      skippedTick = false;
      if (rt && !rt.isBusy()) refreshAll();
    }
  }
  document.addEventListener("visibilitychange", onVisibility);
  unlisten.push(() => document.removeEventListener("visibilitychange", onVisibility));

  // --- the page --------------------------------------------------------------------------------------------------

  const linkContext = () => ({
    from: rt.range.from_ms,
    to: rt.range.to_ms,
    fromText: rt.range.fromText,
    toText: rt.range.toText,
    timezone: pageApi.zone,
    dashboardUid: uid,
    variables: rt.variables.map((v) => ({ name: v.name, kind: v.kind, all: rt.selections.get(v.name).all, values: rt.selections.get(v.name).values })),
    interpolate: (text) => rt.interpolate(text),
  });

  const pageApi = {
    get rt() {
      return rt;
    },
    // The zone of the page (9.9): the known zone of the URL (9.5), else the zone setting, or `browser` when the browser does not know the name.
    get zone() {
      return urlZone || effectiveZone(loaded.doc.time.timezone);
    },
    get syncKey() {
      return loaded.doc.cursor_sync === "crosshair" ? `hune-${uid}` : "";
    },
    // The range and the refresh that the header controls show now (10.9: the Save dialog offers them as the defaults). `refresh` is null for off.
    get currentTime() {
      const r = currentRange();
      return { from: r.fromText, to: r.toText, refresh };
    },
    linkContext,
    panelUrl,
    viewPanel: (id) => dispatch("view-panel", id),
    onZoom(fromMs, toMs) {
      // A drag on a time series sets an absolute time range (9.8).
      const result = rt.setRange(String(fromMs), String(toMs));
      if (result.error) notify({ level: "warning", text: result.error });
      else {
        timePicker.update();
        writeUrl(true);
      }
    },
    // The item "Edit" of the panel menu in the view modes (10.2a, rule 5): the panel editor opens, and "Back" leaves it for this view.
    // A copy of a repeat opens the editor of the stored panel (9.8c).
    extraMenuItems: (panel, inst) => (canEdit() ? [{ separator: true }, { label: "Edit", testid: `panel-${inst ? inst.key : panel.id}-edit`, onSelect: () => dispatch("panel-edit", panel.id) }] : []),
  };

  // A zone name that the browser does not know: the page uses the zone `browser` and shows a warning with the hook `data-notice="zone"` (9.9).
  // The notice shows once for each name, so a reload of the document does not show it again after a viewer closed it.
  function noticeZone(name) {
    if (isKnownZone(name)) {
      if (zoneNoticeFor !== null) dismissNotice("zone");
      zoneNoticeFor = null;
      return;
    }
    if (zoneNoticeFor === name) return;
    zoneNoticeFor = name;
    const node = notify({ level: "warning", key: "zone", text: `The time zone ${name} is not known to this browser. The page uses the browser zone.` });
    if (node) node.dataset.notice = "zone";
  }

  // The member `names` of the answer of 6.14 is absent or wrong (9.10): the runtime runs each query on each change of a variable, and the page says it
  // once, with the hook `data-notice="names"`.
  // The file of the dashboard is not valid now: the server shows the last valid state (11.36). The page says it once, with the hook `data-notice="stale"`.
  function noticeStale() {
    if (!loaded.stale || staleNoticed) return;
    staleNoticed = true;
    const node = notify({ level: "warning", key: "stale", text: STALE_PAGE_TEXT });
    if (node) node.dataset.notice = "stale";
  }

  function noticeNames() {
    if (!rt.namesError || namesNoticed) return;
    namesNoticed = true;
    const node = notify({ level: "warning", key: "names", text: `The server did not give the variable names of the queries (${rt.namesError}). Hune runs every query when a variable changes.` });
    if (node) node.dataset.notice = "names";
  }

  // The group of the dashboard links (9.8a). The lists of the `dashboards` links load once for each load of the dashboard, and two links
  // with the same first tag share one request.
  function buildLinks() {
    const links = (loaded.doc.links || []);
    if (!links.length) return null;
    const pages = new Map();
    const list = (params) => {
      const key = `${params.tag}\u0000${params.after}`;
      if (!pages.has(key)) pages.set(key, api.listDashboards(params, controller.signal));
      return pages.get(key);
    };
    return createLinksBar({
      links,
      ctx: () => linkContext(),
      interpolate: (text) => rt.interpolate(text),
      loadItems: (link) => fetchItems(link, list, uid),
    });
  }

  function buildPage(initialRange, urlVars, keepSelections) {
    loaded = { doc: answer.document, etag: answer.etag, version: answer.version, folder: answer.folder, access: answer.access || { write: false }, readOnly: answer.read_only === true, stale: isStale(answer) };
    const doc = loaded.doc;
    document.title = `${doc.title} - Hune`;
    rt = new Runtime({
      uid,
      doc,
      etag: loaded.etag,
      api,
      // The view of one panel measures its own frame: the frames of the hidden grid have no width.
      widthOf: (key) => {
        if (viewFrame) {
          const view = rt && rt.panels.get(viewFrame.panelId);
          const reuse = view && view.panel.data.reuse;
          if (viewFrame.panelId === key || (reuse && reuse.panel === key)) return viewFrame.frame.width();
        }
        return grid && grid.frames.get(key) ? grid.frames.get(key).width() : 0;
      },
      range: initialRange,
      urlVars,
      keepSelections,
      limits: limitsOf(app.session),
      names: answer.names,
      adhoc: answer.adhoc,
      onNotice: variableNotice,
      zone: pageApi.zone,
    });
    rt.on((type, data) => {
      if (destroyed) return;
      if (type === "panel") {
        const f = grid && grid.frames.get(data.key);
        if (f) f.update();
        if (viewFrame && viewFrame.panelId === data.key) viewFrame.frame.update();
      } else if (type === "annotations") {
        // New marks (9.19d): each frame of a time series draws its own.
        if (grid) for (const f of grid.frames.values()) if (f.annotationsChanged) f.annotationsChanged();
        if (viewFrame) viewFrame.frame.annotationsChanged();
      } else if (type === "instances") {
        // A new list of values of a repeat (9.8c): the copies are drawn again, and rule 16 of 10.2a checks the view of a copy.
        if (grid && !editHooks) grid.sync();
        refreshView();
        checkViewKey();
      } else if (type === "range") {
        // The note of a time override holds the range of the page (9.8b), and the links keep the time (9.8a).
        if (grid) grid.paintTexts();
        if (viewFrame) viewFrame.frame.paintTexts();
        if (linksBar) linksBar.update();
      } else if (type === "vars") {
        if (bar) bar.update();
        if (linksBar) linksBar.update();
        // The titles, the descriptions and the section titles hold variables (9.13).
        if (grid) grid.paintTexts();
        if (viewFrame) viewFrame.frame.paintTexts();
      } else if (type === "stale") {
        // In edit mode the page does not read the document again (10.8).
        if (editHooks && editHooks.onStale) editHooks.onStale(data);
        else reloadDocument();
      }
      // The run button shows a spinner while the queries run.
      if (refreshPicker && rt && (type === "panel" || type === "annotations" || type === "range" || type === "vars" || type === "idle")) refreshPicker.setRunning(rt.isBusy());
    });
    const makeFrame = (panel, inst) => createPanelFrame({ panel, page: pageApi, instance: inst });
    // A variable with a placement has its control in the grid (4.7a): the grid gets it as a placed panel (var_layout.js).
    grid = createGrid({ doc: gridDoc(doc), rt, makeFrame });
    if (!urlZone) noticeZone(doc.time.timezone);
    noticeNames();
    noticeStale();

    timeControl = createTimeControl({
      time: {
        zone: rt.zone,
        getRange: currentRange,
        apply: (fromText, toText) => {
          const result = rt.setRange(fromText, toText);
          if (!result.error) {
            timePicker.update();
            writeUrl();
          }
          return result;
        },
        onZoomOut: () => moveRange(zoomOut(rt.range.from_ms, rt.range.to_ms)),
        // Back and forward: the range moves by half of its span, as in Grafana (time.js shiftRange).
        onShift: (dir) => moveRange(shiftRange(rt.range.from_ms, rt.range.to_ms, dir)),
      },
      refresh: {
        choices: REFRESH_CHOICES,
        value: refresh,
        onChange: (value) => {
          setRefresh(value);
          writeUrl();
        },
        onRefreshNow: () => refreshAll(),
      },
    });
    timePicker = timeControl.time;
    refreshPicker = timeControl.refresh;
    bar = createVarsBar(rt, (name, selection) => {
      rt.setSelection(name, selection);
      writeUrl();
    });
    renderHeader();
    syncBar();
    clear(main);
    viewHost = el("div", { class: "view-panel", hidden: true, tabindex: "-1", role: "region", "aria-label": "View panel", dataset: { testid: "view-panel" } });
    editHost = el("div", { class: "editor-host", dataset: { testid: "editor-host" } });
    main.append(el("h1", { class: "visually-hidden", text: doc.title }), toolbar, grid.el, viewHost, editHost);
  }

  // The variables bar is useless on the JSON tab, the history, a version and a diff: it hides there. It stays in the view,
  // the edit mode and the panel editor, whose preview uses the selections.
  function syncBar() {
    // The panel editor and the other editor views take the row of the variables out of the flow (static). This applies also to a
    // dashboard without variables.
    if (toolbar) toolbar.classList.toggle("dash-toolbar--static", shown.page === "panel-edit" || shown.page === "json" || shown.page === "settings" || shown.page === "history");
    if (!bar) return;
    const away = shown.page === "json" || shown.page === "settings" || shown.page === "history";
    bar.el.classList.toggle("vars-bar--off", away);
    if (linksBar) linksBar.el.classList.toggle("dash-links--off", away);
  }

  // Moves the range to an absolute range (zoom out, back, forward) and writes the address.
  function moveRange(d) {
    const result = rt.setRange(String(d.from_ms), String(d.to_ms));
    if (result.error) notify({ level: "warning", text: result.error });
    else {
      timePicker.update();
      writeUrl();
    }
  }

  // The header is one row (main.js, 9.6). From the left: the brand, the selector of the dashboards, the group of the edit mode (Add, Save, Revert), the time
  // control (the range, the refresh and the interval, joined), the switch "Edit", the theme selector and the button "More actions" (9.9).
  // Only the brand and the selector change width. The variables are in their own row under the header (css .dash-toolbar).
  function canEdit() {
    return session.editing === true && loaded.access && loaded.access.write === true;
  }

  // The switch "Edit" (10.2). It is on while the editor holds the draft. A press turns the editor on, or off with the question about the
  // changes. A native button has the keys Space and Enter.
  function buildEditSwitch() {
    const node = el("button", { type: "button", role: "switch", class: "btn btn--tb edit-switch", "data-tooltip": "Edit", dataset: { testid: "edit-toggle" } },
      el("span", { class: "edit-switch__label btn__label", text: "Edit" }),
      el("span", { class: "edit-switch__track", "aria-hidden": "true" }, el("span", { class: "edit-switch__thumb" })));
    node.addEventListener("click", () => dispatch(state.editing ? "edit-off" : "edit-on"));
    return node;
  }

  function paintEditSwitch() {
    if (!editSwitch) return;
    editSwitch.setAttribute("aria-checked", String(editActive));
    editSwitch.classList.toggle("is-on", editActive);
  }

  // The items of the menu "More actions": the time shift and the zoom out when the window is narrow (the buttons hide), the actions of the
  // edit mode, then History, Export and Delete. An item shows only when the user has the right for it (9.9).
  function moreItems() {
    const items = [];
    if (timeInMenu()) {
      items.push({ label: "Shift the time range back", icon: "chevrons-left", testid: "menu-time-back", onSelect: () => moveRange(shiftRange(rt.range.from_ms, rt.range.to_ms, -1)) });
      items.push({ label: "Shift the time range forward", icon: "chevrons-right", testid: "menu-time-forward", onSelect: () => moveRange(shiftRange(rt.range.from_ms, rt.range.to_ms, 1)) });
      items.push({ label: "Zoom out", icon: "zoom-out", testid: "menu-zoom-out", onSelect: () => moveRange(zoomOut(rt.range.from_ms, rt.range.to_ms)) });
      items.push({ separator: true });
    }
    const editing = editHooks && editHooks.menuItems ? editHooks.menuItems() : [];
    if (editing.length) items.push(...editing, { separator: true });
    items.push({ label: "History", icon: "history", testid: "history-button", onSelect: () => dispatch("open-history") });
    items.push({ label: "Export", icon: "download", testid: "export-button", onSelect: () => exportDashboard(uid) });
    if (canEdit()) {
      const item = { uid, title: loaded.doc.title, etag: loaded.etag, folder: loaded.folder };
      items.push({ separator: true });
      items.push({ label: "Delete", icon: "trash", danger: true, testid: "delete-button", onSelect: () => editorCall("folders", "deleteDashboard", item) });
    }
    return items;
  }

  function renderHeader() {
    header.clear();
    paintLocation();
    const more = button("More actions", { "aria-haspopup": "menu", "aria-expanded": "false", dataset: { testid: "dashboard-menu", tooltipAlign: "end" } }, "more", { iconOnly: true, variant: "tb" });
    more.addEventListener("click", () => openMenu(more, moreItems(), { align: "end", gap: 8 }));
    editSwitch = null;
    editBar = null;
    // The switch shows only with the right to write. The group of the edit mode takes room from the selector only.
    if (canEdit()) {
      editSwitch = buildEditSwitch();
      paintEditSwitch();
    }
    // A dashboard of a read-only store has no switch "Edit" (its access.write is false: the path of a user with no right to write). A lock stands
    // where the switch would be, with its name for assistive technology and a tooltip.
    const lock = loaded.readOnly
      ? el("span", { class: "header-lock", role: "img", tabindex: "0", "aria-label": READ_ONLY_TEXT, "data-tooltip": READ_ONLY_TEXT, "data-tooltip-align": "end", dataset: { testid: "read-only-lock" } }, icon("lock", 18))
      : null;
    header.setActions(timeControl.el, lock, editSwitch);
    header.setMore(more);
    linksBar = buildLinks();
    toolbar = el("div", { class: "dash-toolbar", dataset: { testid: "dashboard-toolbar" } }, bar.el, ...(linksBar ? [linksBar.el] : []));
    if (typeof ResizeObserver === "function") {
      const watch = new ResizeObserver(syncViewHeight);
      watch.observe(toolbar);
      unlisten.push(() => watch.disconnect());
    }
  }

  // The view of one panel ends above the window edge: it needs the height of the row of the variables (css .view-panel).
  // The row of the variables has a negative top margin: it lies over the padding of the page. The space that it takes under the padding is its
  // height plus this margin, so the view of a panel subtracts that space and ends one page padding above the window edge.
  function syncViewHeight() {
    if (!viewHost || !toolbar) return;
    const lift = Number.parseFloat(getComputedStyle(toolbar).marginTop) || 0;
    viewHost.style.setProperty("--view-toolbar-h", `${Math.max(0, toolbar.offsetHeight + lift)}px`);
  }

  // The place of the page in the header (9.6): the selector of the dashboards and, in the view of one panel or the panel editor, a way back
  // (an arrow) and the name of that view. The way back of the panel editor is the event `leave`; the way back of the view of a panel is a link.
  function dashboardUrlFor(edit) {
    return relativeUrl(edit ? { ...baseState(), page: "edit", editing: true } : baseState());
  }

  function paintLocation() {
    // The view of a panel and the panel editor have the way back, an arrow at the right of the selector, and no name.
    const inView = rt && Boolean(viewTails.edit || viewTails.view);
    const back = inView
      ? { href: dashboardUrlFor(Boolean(viewTails.edit)), label: "Back to dashboard", attrs: { dataset: { testid: "view-back" } }, ...viewTails.back }
      : null;
    header.setLocation(...locationNodes({ uid, title: loaded.doc.title, navigate: ctx.navigate, back, session }));
  }

  // --- the states of the page (10.2a) -------------------------------------------------------------------------------------------

  const editDirty = () => Boolean(editHooks && editHooks.dirty && editHooks.dirty());

  // The ids of the panels that a panel editor can open: those of the draft when the editor holds it, else of the stored draft, else of the saved document.
  function draftPanelIds() {
    const held = editHooks && editHooks.panelIds ? editHooks.panelIds() : null;
    if (held) return held;
    try {
      const raw = window.localStorage.getItem(`hune.draft.v1.${uid}`);
      const sections = raw === null ? null : JSON.parse(raw).document.sections;
      if (Array.isArray(sections)) return sections.flatMap((section) => section.panels.map((panel) => panel.id));
    } catch (error) {
      // The saved document answers.
    }
    return [...rt.stored.keys()];
  }

  const parseCtx = () => ({ canEdit: canEdit(), viewPanels: [...rt.stored.keys()], editPanels: draftPanelIds(), repeatLimits: repeatLimits(rt.doc, rt.limits), plan: rt.plan });

  // The origin of the current entry (10.2a): the state of the entry that opened it, or null. A pasted URL, a link and a redirect have none.
  function currentOrigin() {
    const from = readOrigin(window.history.state);
    if (from === null || !(from.startsWith(`d/${uid}/`) || from.startsWith(`d/${uid}?`) || from === `d/${uid}`)) return null;
    const at = from.indexOf("?");
    const origin = parseView(at < 0 ? "" : from.slice(at), parseCtx()).state;
    return { kind: kindOf(origin), state: origin };
  }

  // The hooks of 9.29 and the parts of the page that follow the state that is in the page.
  function settle(target) {
    shown = target;
    syncBar();
    // A chart that returns to the page draws in this task, before the hook says that the view shows (10.2a, rule 17).
    document.dispatchEvent(new Event("hune-shown"));
    const root = document.documentElement;
    root.setAttribute("data-hune-view", target.page);
    const origin = isEditorState(target) ? currentOrigin() : null;
    if (origin) root.setAttribute("data-hune-origin", origin.kind);
    else root.removeAttribute("data-hune-origin");
  }

  const hookOf = (node) => (node && node.dataset && node.dataset.testid) || null;

  // Rule 14: the focus goes back to the control that opened the state, else to the region of the new state.
  function giveFocusBack() {
    if (!focusBack) return;
    focusBack = false;
    if (shown.page === "panel-view") return; // the view takes the focus (openView)
    const node = opener ? document.querySelector(`[data-testid="${opener}"]`) : null;
    if (node && node.offsetParent !== null && typeof node.focus === "function") whenShown(() => node.focus({ preventScroll: true }));
  }

  // Shows the view of the state in the page: one view, with no frame between (rule 2). It returns when the view is in the page.
  async function showState() {
    const target = state;
    if (isEditorState(target)) {
      await openEditor(target);
      return;
    }
    // The editor closes, and the panel view opens or closes, in one step.
    if (editHooks && editHooks.close) editHooks.close();
    if (target.page === "panel-view") {
      if (!viewFrame || viewFrame.panelId !== target.panel) openView(target.panel);
    } else closeView();
    settle(target);
    giveFocusBack();
  }

  // Does one change of the state: the URL (push, replace or pop), then the view (10.2a).
  function perform(out) {
    state = out.state;
    if (out.history === "push") writeEntry(out.state, "push", out.writeOrigin);
    else if (out.history === "replace") writeEntry(out.state, "replace", false);
    else if (out.history === "pop") popKeepingQuery();
    return showState();
  }

  // An event of 10.2a. The table of view_state.js gives the next state. The dialog of 10.2 shows first when the event ends edit mode with changes.
  function dispatch(event, arg, extra = {}) {
    if (!rt || destroyed) return Promise.resolve();
    const out = nextState(state, event, arg, { origin: currentOrigin(), dirty: editDirty(), ...extra });
    if (out.history === "none") return Promise.resolve();
    if (event === "leave" || event === "close-view" || event === "up") {
      focusBack = true;
      opener = openers.get(`${state.page}:${state.panel ?? ""}`) || null;
    } else openers.set(`${out.state.page}:${out.state.panel ?? ""}`, hookOf(document.activeElement) || "dashboard-menu");
    const from = state;
    // After "Save" in the dialog the page has left the state already: the event does nothing then.
    const go = () => (state === from ? perform(out) : Promise.resolve());
    if (out.dialog && editHooks && editHooks.confirmLeave) {
      editHooks.confirmLeave(go);
      return Promise.resolve();
    }
    return go();
  }

  // The end of edit mode after a save, or the end of a restore of the history: the page reads the document again, and the editor goes away
  // in the same step as the new page appears (10.2a, rule 7).
  async function finishEdit(event) {
    if (!rt || destroyed) return;
    const out = nextState(state, event, undefined, { origin: currentOrigin(), dirty: false });
    const target = out.history === "none" ? (isEditorState(state) ? { ...baseState() } : state) : out.state;
    focusBack = true;
    await reloadDocument({ notice: false, to: target, op: out.history, before: () => {
      if (!isEditorState(target) && editHooks && editHooks.close) editHooks.close();
      if (out.history === "pop") popKeepingQuery();
    } });
  }

  // The notices of the parse of a URL (10.2a, rules 4 and 8).
  function showNotices(notices) {
    for (const n of notices) {
      if (n.code === "no-right") {
        const draft = hasStoredDraft();
        notify({ level: "warning", key: "state-no-right", text: draft ? "You cannot edit this dashboard. Your draft stays in this browser." : "You cannot edit this dashboard." });
      } else if (n.code === "no-panel") notify({ level: "warning", key: `state-no-panel-${n.id}`, text: `This dashboard has no panel ${n.id}.` });
      else if (n.code === "bad-value") notify({ level: "warning", key: `state-bad-${n.name}`, text: `The address has a value that Hune does not use: ${n.name}=${n.value}.` });
    }
  }

  function hasStoredDraft() {
    try {
      return window.localStorage.getItem(`hune.draft.v1.${uid}`) !== null;
    } catch (error) {
      return false;
    }
  }

  // Rule 4: the entry that the page shows has the normal form of its URL. Only the names that the state does not use go. The entry keeps its history.state.
  function normalise(target) {
    const search = normalSearch(window.location.search, target);
    if (search !== window.location.search && stillHere()) window.history.replaceState(window.history.state, "", `${window.location.pathname}${search}`);
  }

  // The API that the editor gets (10.2). The editor modules load on demand.
  const editorApi = {
    uid,
    session,
    header,
    main,
    navigate: ctx.navigate,
    get loaded() {
      return loaded;
    },
    get runtime() {
      return rt;
    },
    get grid() {
      return grid;
    },
    get editHost() {
      return editHost;
    },
    // The state of the page that the editor has to show (10.2a), and the origin of the current entry.
    get state() {
      return state;
    },
    get origin() {
      return currentOrigin();
    },
    get pageApi() {
      return pageApi;
    },
    linkContext,
    dispatch,
    finishEdit,
    whenShown,
    // The editor tells the page that the view of the state is in the page: the page follows (the header, the row of the variables, the hooks).
    shown(target) {
      settle(target);
      giveFocusBack();
    },
    setEditHooks(hooks) {
      editHooks = hooks;
    },
    // The editor puts its group before the time control, or takes it out with null (10.2).
    setEditBar(node) {
      if (editBar === node) return;
      if (editBar) editBar.remove();
      editBar = node;
      if (node && editSwitch) timeControl.el.before(node);
    },
    // The editor tells the page that it holds the draft (true) or not: the switch "Edit" follows.
    setEditActive(on) {
      editActive = Boolean(on);
      paintEditSwitch();
    },
    // The panel editor takes the toolbar row into its own layout (it sits under the top bar) and gives it back.
    dockToolbar(parent, before = null) {
      if (toolbar) parent.insertBefore(toolbar, before);
    },
    undockToolbar() {
      const h1 = main.querySelector(":scope > h1");
      if (toolbar && h1 && toolbar.parentElement !== main) h1.after(toolbar);
    },
    // The header while the editor shows a view of a panel: `editing` is true for the panel editor, which has the way back and no name. `back` gives
    // the label and the attributes (test id, click) of the way back.
    setViewTail(editing, back) {
      viewTails = { ...viewTails, edit: editing === true ? true : null, back: back || null };
      paintLocation();
    },
    // Hides the base grid (the editor draws the draft grid) and shows it again. The view of a panel closes first: one view at a time (10.2a, rule 2).
    pauseBase() {
      closeView();
      grid.suspendFrames();
      grid.pauseObserver();
      // The grid leaves the document, so that the hooks of its panels (data-testid) are unique while the editor shows its own.
      gridMarker = document.createComment("grid");
      grid.el.replaceWith(gridMarker);
    },
    resumeBase() {
      if (gridMarker && gridMarker.isConnected) gridMarker.replaceWith(grid.el);
      gridMarker = null;
      grid.hide(false);
      grid.resumeObserver();
      grid.sync();
      grid.resumeFrames();
    },
    // Reads the document again and builds the page (after a restore or "Load theirs").
    reload: (opts) => reloadDocument(opts),
    notify,
    writeUrl,
  };

  // Opens the editor, or shows another view of it. It returns when the view is in the page.
  async function openEditor(target) {
    const mod = await loadEditorModule("edit_mode");
    if (!mod || destroyed) return;
    const enter = mod.enter || mod.default;
    if (typeof enter !== "function") {
      notify({ level: "warning", text: "The editor is not available in this build." });
      return;
    }
    await enter(editorApi, target);
  }

  async function editorCall(name, fn, ...args) {
    const mod = await loadEditorModule(name);
    if (mod && typeof mod[fn] === "function") await mod[fn]({ session, navigate: ctx.navigate, reload: () => reloadDocument({ notice: false }), isDashboardPage: true }, ...args);
  }

  // --- view of one panel (9.8) ----------------------------------------------------------------------------------------

  // Puts the view of the panel in the page. It writes no URL: the state does (10.2a).
  // `id` is the key of a panel or of a copy of a repeat (9.8c). While the options of the repeat load, a copy has a record in the state `idle`.
  function openView(id) {
    const panel = rt.panels.get(id) || rt.ensureView(id);
    if (!panel) return;
    closeView();
    grid.suspendFrames();
    grid.hide(true);
    // The key of the record: a form of Grafana that waits shows its panel (rule 16).
    const frame = createPanelFrame({ panel: panel.panel, page: pageApi, key: panel.key });
    frame.el.style.gridColumn = "";
    frame.el.style.gridRow = "";
    frame.el.classList.add("panel--view");
    clear(viewHost);
    viewHost.append(frame.el);
    viewHost.hidden = false;
    syncViewHeight();
    viewFrame = { frame, panelId: id, record: panel };
    rt.setViewPanel(id);
    frame.update(true);
    for (const other of rt.panels.values()) if (other.panel.data.reuse && other.panel.data.reuse.panel === id) frame.update(true);
    viewTails = { ...viewTails, view: true };
    paintLocation();
    // The focus moves to the view, so that Tab starts in the panel and a screen reader names the region. The view shows no ring (css)
    // and, being outside the frame, it does not show the menu button or the edge of the frame.
    whenShown(() => viewHost.focus({ preventScroll: true }));
  }

  function closeView() {
    if (!viewFrame && viewHost.hidden) return;
    if (viewFrame) {
      viewFrame.frame.destroy();
      viewFrame = null;
    }
    clear(viewHost);
    viewHost.hidden = true;
    viewTails = { ...viewTails, view: null };
    paintLocation();
    grid.hide(false);
    grid.sync();
    grid.resumeFrames();
    rt.setViewPanel(null);
  }

  // The record of the viewed copy is new after a change of the values: the view asks for its data again (9.8c).
  function refreshView() {
    if (!viewFrame || !rt) return;
    const record = rt.panels.get(viewFrame.panelId);
    if (!record || record === viewFrame.record) return;
    viewFrame.record = record;
    rt.setViewPanel(viewFrame.panelId);
    viewFrame.frame.paintTexts();
    viewFrame.frame.update(true);
  }

  // Rule 16 of 10.2a: a view of a copy that the values do not make is removed with a notice, once the values are known. The form of Grafana
  // then gets the key of the copy of its value (rule 1).
  function checkViewKey() {
    if (!rt || destroyed || state.page !== "panel-view") return;
    const { id, value } = splitKey(state.panel);
    if (rt.repeatLoading(id)) return;
    const key = value === undefined ? state.panel : copyKeyOf(rt.plan, id, value);
    const record = rt.panels.get(key);
    const found = record && !record.provisional;
    if (found && key === state.panel) return;
    if (found) state = { ...state, panel: key };
    else {
      notify({ level: "warning", key: `state-no-panel-${state.panel}`, text: `This dashboard has no panel ${state.panel}.` });
      state = baseState();
    }
    writeEntry(state, "replace", false);
    showState();
  }

  const onKey = (event) => {
    if (event.key === "Escape" && state.page === "panel-view" && !document.querySelector("dialog[open], .menu, .picker__popup, .select__popup, .filter-popup")) dispatch("close-view");
  };
  document.addEventListener("keydown", onKey);
  unlisten.push(() => document.removeEventListener("keydown", onKey));

  // --- 409 dashboard_changed (7.15, 9.8) -----------------------------------------------------------------------------------

  let reloading = false;
  // Reads the document and builds the page again. `to` is the state that the new page shows (after a save: the end of edit mode); `before` runs in the
  // same step as the new page, so no frame shows the old page and the new page. `op` is the history action of `to` (10.2a).
  async function reloadDocument({ notice = true, to = null, op = "none", before = null } = {}) {
    if (reloading || destroyed) return;
    reloading = true;
    try {
      const keep = selectionsOf(rt);
      const range = currentRange();
      const fresh = await api.dashboard(uid, controller.signal);
      if (destroyed) return;
      if (before) before();
      answer = fresh;
      teardownPage();
      if (to) state = to;
      buildPage(range, new Map(), keep);
      if (editHooks && editHooks.rebuilt) editHooks.rebuilt();
      else if (state.page === "panel-view" && (rt.panels.has(state.panel) || rt.ensureView(state.panel))) {
        openView(state.panel);
        settle(state);
      } else {
        if (state.page === "panel-view") state = baseState();
        settle(state);
      }
      if (!to || op === "replace") writeUrl();
      if (notice) notify({ level: "info", text: "The dashboard changed: Hune loaded the new version." });
      await rt.start(new Map());
      // The options of the variables are loaded now, so their defaults are known: the address drops the names that equal their default (9.5).
      if (!destroyed && rt) writeUrl();
    } catch (error) {
      if (!api.isAbort(error)) notify({ level: "error", text: `The dashboard did not reload: ${error.message}` });
    } finally {
      reloading = false;
    }
  }

  function teardownPage() {
    if (viewFrame) {
      viewFrame.frame.destroy();
      viewFrame = null;
    }
    viewTails = { ...viewTails, view: null };
    if (rt) rt.destroy();
    if (grid) grid.destroy();
    if (linksBar) linksBar.destroy();
    rt = grid = bar = linksBar = toolbar = editBar = editSwitch = timeControl = null;
  }

  // --- location (Back, Forward and links inside the dashboard, 9.5) ---------------------------------------------------------------------

  function locationChanged() {
    if (!rt || destroyed) return;
    const s = parseSearch(window.location.search);
    // The end of a view by a Back that the page made: its own range, refresh and variables stay (see popKeepingQuery). The carry lasts 3 s.
    const carried = carriedQuery && Date.now() - carriedQuery.at < 3000 ? carriedQuery.query : null;
    carriedQuery = null;
    if (carried) {
      s.from = carried.from;
      s.to = carried.to;
      s.refresh = carried.refresh;
      s.vars = carried.vars;
    }
    // Range and refresh.
    const from = s.from !== null ? s.from : loaded.doc.time.from;
    const to = s.to !== null ? s.to : loaded.doc.time.to;
    const r = currentRange();
    if (from !== r.fromText || to !== r.toText) {
      const result = rt.setRange(from, to);
      if (result.error) notify({ level: "warning", text: result.error });
      else timePicker.update();
    }
    const nextRefresh = refreshFromUrl(s.refresh);
    if (nextRefresh !== refresh) setRefresh(nextRefresh);
    // Variables.
    for (const v of rt.variables) {
      const values = s.vars.get(v.name);
      const cur = rt.selections.get(v.name);
      // A name that the URL does not have means the default selection (9.5). A default that is not known yet changes nothing.
      const sel = values ? selectionFromUrl(v, values) : rt.defaultSelection(v.name);
      if (sel && !sameSelection(sel, cur)) rt.setSelection(v.name, sel);
    }
    // The state of the URL (10.2a, rule 1). Back and Forward never show a dialog: the dialogs close, and the draft stays (rule 9).
    const parsed = parseView(window.location.search, parseCtx());
    const next = parsed.state;
    if (!sameState(next, state)) {
      const leftEdit = state.editing && !next.editing;
      closeMenu({ restoreFocus: false });
      for (const dialog of document.querySelectorAll("dialog[open]")) dialog.close();
      state = next;
      focusBack = false;
      showState().then(() => {
        if (leftEdit && !isEditorState(state)) draftNotice();
      });
    } else settle(state);
    showNotices(parsed.notices);
    normalise(next);
    if (carried) writeUrl();
    else document.dispatchEvent(new Event("hune-query"));
  }

  function selectionFromUrl(v, values) {
    if (v.kind === "constant") return null; // a constant never reads the URL (4.10b, rule 3)
    if (v.kind === "text") return { all: false, values: [values[0] ?? ""] };
    if (values.includes("$__all")) return v.include_all ? { all: true, values: [] } : null;
    return { all: false, values: v.multi ? values : values.slice(0, 1) };
  }

  function refreshFromUrl(value) {
    if (value === null || value === undefined) return loaded.doc.time.refresh;
    if (value === "off") return null;
    if (REFRESH_CHOICES.includes(value)) return value;
    notify({ level: "warning", key: "refresh-url", text: `The refresh value ${value} is not offered. Refresh is off.` });
    return null;
  }

  // A dashboard with a draft shows a notice with "Resume" and "Discard" (10.8). The draft module loads only when a draft exists.
  // The notice shows in the states `view` and `panel-view`, to a user who can edit (10.2a, rule 8). "Resume" opens the state `edit`.
  async function draftNotice() {
    if (!canEdit() || (state.page !== "view" && state.page !== "panel-view")) return;
    let raw = null;
    try {
      raw = window.localStorage.getItem(`hune.draft.v1.${uid}`);
    } catch (error) {
      return;
    }
    if (raw === null) return;
    const mod = await loadEditorModule("draft_store");
    if (!mod || destroyed) return;
    mod.showResumeNotice(editorApi, raw, () => {
      if (destroyed || (state.page !== "view" && state.page !== "panel-view")) return;
      openers.set("edit:", "dashboard-menu");
      perform({ state: { ...baseState(), page: "edit", editing: true }, history: "push", writeOrigin: true });
    });
  }

  // --- start ----------------------------------------------------------------------------------------------------------------------

  const s0 = parseSearch(window.location.search);
  const doc0 = answer.document;
  // The zone of the URL (9.5): a known name applies to the view, an unknown name gives a notice and the zone of the document.
  if (isKnownZone(s0.timezone)) urlZone = s0.timezone;
  else if (s0.timezone !== null) {
    const node = notify({ level: "warning", key: "zone-url", text: `The time zone “${cut(s0.timezone)}” of the address is not known to this browser. The page uses the zone of the dashboard.` });
    if (node) node.dataset.notice = "zone-url";
  }
  let initial = { fromText: doc0.time.from, toText: doc0.time.to };
  if (s0.from !== null || s0.to !== null) {
    const candidate = { fromText: s0.from !== null ? s0.from : doc0.time.from, toText: s0.to !== null ? s0.to : doc0.time.to };
    const check = resolveRange(candidate.fromText, candidate.toText, Date.now(), urlZone || effectiveZone(doc0.time.timezone));
    if (check.error) notify({ level: "warning", text: `The time range of the address is not used: ${check.error}` });
    else initial = candidate;
  }
  loaded = { doc: doc0 };
  refresh = doc0.time.refresh;
  if (s0.refresh !== null) refresh = refreshFromUrl(s0.refresh);
  const urlVars = new Map();
  for (const v of doc0.variables) {
    const values = s0.vars.get(v.name);
    if (values) urlVars.set(v.name, values);
  }
  buildPage(initial, urlVars, null);
  setRefresh(refresh);
  // The state of the URL (10.2a, rule 1), and its normal form (rule 4). The page writes its own slug in the URL (9.5). The query stays as it is until
  // the first change.
  const parsed0 = parseView(window.location.search, parseCtx());
  state = parsed0.state;
  if (stillHere()) window.history.replaceState(window.history.state, "", `${new URL(document.baseURI).pathname}${dashboardPath(uid, doc0.title)}${normalSearch(window.location.search, state)}`);
  const startPromise = rt.start(urlVars);
  busy.begin();
  startPromise.finally(busy.end);
  // A view of a copy that the values do not make is removed before the first paint (rule 16 of 10.2a).
  checkViewKey();
  // The first paint stays held until the view of the state is in the page, also for an editor state (9.6, 10.2a).
  await showState();
  showNotices(parsed0.notices);
  draftNotice();

  return {
    locationChanged,
    destroy() {
      destroyed = true;
      controller.abort();
      clearInterval(refreshTimer);
      for (const off of unlisten) off();
      dismissNotice("refresh-url");
      dismissNotice("zone");
      dismissNotice("zone-url");
      document.documentElement.removeAttribute("data-hune-view");
      document.documentElement.removeAttribute("data-hune-origin");
      if (editHooks && editHooks.destroy) editHooks.destroy();
      teardownPage();
    },
  };
}
