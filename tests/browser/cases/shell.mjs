// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B49 to B54 (9.6, 9.7, 9.9, 9.25): the header bar, the theme button and the version line.
// The controls of the home page and of the dashboard page are in the header. The theme button is the same on every page.
// The header is one row of 44 px at every width (wave K): History, Export and Delete and the actions of the edit mode are in the menu "More actions".
import { enterEdit } from "../lib/ed.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";

const HEADER = "#app-header";
const VERSION_RE = /^\d+\.\d+\.\d+[0-9A-Za-z.+_-]*:[0-9a-f]{9}$/;

/** Sets the size of the viewport of a page and waits for the layout of the header. */
async function size(p, width, height = 900) {
  await p.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  await sleep(250);
}

/** The test ids of the controls that are inside the header, inside the main area, and those that are not in the page. */
const where = (p, ids) =>
  p.eval(`(() => {
    const header = document.querySelector("${HEADER}");
    const main = document.querySelector("#app-main");
    const out = { header: [], main: [], missing: [] };
    for (const id of ${JSON.stringify(ids)}) {
      const node = document.querySelector('[data-testid="' + id + '"]');
      if (!node) out.missing.push(id);
      else if (header.contains(node)) out.header.push(id);
      else if (main.contains(node)) out.main.push(id);
    }
    return out;
  })()`);

/** True when the page has no horizontal scroll. */
const noOverflow = (p) => p.eval(`document.documentElement.scrollWidth <= document.documentElement.clientWidth`);

/** The height of the header and the number of rows of its controls (the tops of the buttons, for a message). */
const rowInfo = (p) =>
  p.eval(`(() => {
    const h = document.querySelector("${HEADER}");
    const tops = [...h.querySelectorAll("button, input")].filter((e) => e.offsetParent !== null).map((e) => Math.round(e.getBoundingClientRect().top + e.getBoundingClientRect().height / 2));
    return { height: Math.round(h.getBoundingClientRect().height), centres: [...new Set(tops)], width: h.clientWidth, scroll: h.scrollWidth };
  })()`);

/** True when each control shows and its box is inside the box of the header. */
const insideHeader = (p, ids) =>
  p.eval(`${JSON.stringify(ids)}.every((id) => {
    const e = document.querySelector('[data-testid="' + id + '"]').getBoundingClientRect();
    const h = document.querySelector("${HEADER}").getBoundingClientRect();
    return e.width > 0 && e.left >= h.left - 0.5 && e.right <= h.right + 0.5 && e.top >= h.top - 0.5 && e.bottom <= h.bottom + 0.5;
  })`);

/** The visible state of the theme button: its name, its tooltip, the icon that shows, and the theme. The button has no menu. */
const themeState = (p) =>
  p.eval(`(() => {
    const trigger = document.querySelector('[data-testid="theme-toggle"]');
    const icons = [...trigger.querySelectorAll(".theme-icon")].filter((e) => getComputedStyle(e).display !== "none").map((e) => e.getAttribute("class").replace(/.*theme-icon--/, ""));
    return {
      label: trigger.getAttribute("aria-label"),
      tooltip: trigger.getAttribute("data-tooltip"),
      icons,
      theme: document.documentElement.dataset.theme,
      choice: document.documentElement.dataset.themeChoice,
      stored: localStorage.getItem("hune.theme"),
    };
  })()`);

/** The size and the place of the selector in the header, for the comparison between pages. */
const shape = (p) =>
  p.eval(`(() => {
    const header = document.querySelector("${HEADER}").getBoundingClientRect();
    const b = document.querySelector('[data-testid="theme-toggle"]').getBoundingClientRect();
    // The button "More actions" of a dashboard follows the selector (9.25): its room is not part of the place of the selector.
    const more = document.querySelector(".header-more");
    const moreW = more && more.offsetWidth ? more.offsetWidth + 8 : 0;
    return { w: Math.round(b.width), h: Math.round(b.height), right: Math.round(header.right - b.right) - moreW, top: Math.round(b.top - header.top), count: document.querySelectorAll('[data-testid="theme-toggle"]').length };
  })()`);

// One click of the button gives the next choice: System, Dark, Light, then System again (9.25). `choose` clicks until the choice is the wanted one.
async function choose(p, value) {
  for (let i = 0; i < 3; i++) {
    if (await p.eval(`(document.documentElement.dataset.themeChoice || "system") === ${JSON.stringify(value)}`)) return;
    await p.click(p.tid("theme-toggle"));
  }
}

export const cases = [
  {
    id: "B49",
    name: "theme button: one icon button, a click gives System, Dark, Light and System again, it has no menu, and the choice persists",
    async run(t) {
      const p = await t.page();
      await p.emulateColorScheme("dark");
      await p.goto(t.url(""));
      await p.idle();
      // The first state: the choice System, and the icon of the choice shows before any click.
      let s = await themeState(p);
      t.eq([s.label, s.tooltip, s.icons, s.theme, s.choice], ["Theme: System", "Theme: System", ["system"], "dark", "system"], "system at the first load");
      // The button has no menu and no popup, before and after a click.
      const noMenu = () => p.eval(`[document.querySelectorAll('[data-testid="theme-menu"], [data-testid="theme-select"], .theme-select__menu, .menu').length, document.querySelector('[data-testid="theme-toggle"]').hasAttribute("aria-haspopup"), document.querySelector('[data-testid="theme-toggle"]').hasAttribute("aria-expanded")]`);
      t.eq(await noMenu(), [0, false, false], "no menu before a click");
      // One click gives the next choice, with its name, its tooltip, its icon and its theme.
      await p.click(p.tid("theme-toggle"));
      s = await themeState(p);
      t.eq([s.label, s.tooltip, s.icons, s.theme, s.choice, s.stored], ["Theme: Dark", "Theme: Dark", ["dark"], "dark", "dark", "dark"], "the first click: dark");
      t.eq(await noMenu(), [0, false, false], "no menu after a click");
      await p.click(p.tid("theme-toggle"));
      s = await themeState(p);
      t.eq([s.label, s.tooltip, s.icons, s.theme, s.choice, s.stored], ["Theme: Light", "Theme: Light", ["light"], "light", "light", "light"], "the second click: light");
      await p.click(p.tid("theme-toggle"));
      s = await themeState(p);
      t.eq([s.label, s.icons, s.theme, s.choice, s.stored], ["Theme: System", ["system"], "dark", "system", "system"], "the third click: system again, and it follows the dark preference");
      // A dark choice does not follow the system, and a system choice follows it.
      await choose(p, "dark");
      await p.emulateColorScheme("light");
      await sleep(150);
      t.eq((await themeState(p)).theme, "dark", "a dark choice does not follow the system");
      await choose(p, "system");
      s = await themeState(p);
      t.eq([s.label, s.icons, s.theme, s.stored], ["Theme: System", ["system"], "light", "system"], "system follows the light preference");
      // Persistence: a new load keeps the choice, against the system.
      await choose(p, "dark");
      await p.reload();
      await p.idle();
      s = await themeState(p);
      t.eq([s.label, s.icons, s.theme, s.choice], ["Theme: Dark", ["dark"], "dark", "dark"], "the choice stays after a reload");
      // The keyboard: Enter on the button gives the next choice, and the focus stays on the button.
      await p.focus(p.tid("theme-toggle"));
      await p.press("Enter");
      s = await themeState(p);
      t.eq([s.label, s.choice], ["Theme: Light", "light"], "Enter gives the next choice");
      t.eq(await p.eval(`document.activeElement.dataset.testid`), "theme-toggle", "the focus stays on the button");
      t.eq(await noMenu(), [0, false, false], "no menu after the key");
    },
  },

  {
    id: "B50",
    name: "theme button: the same control, at the same place (before the button More actions of a dashboard), on the list, a dashboard, the editor and the pages of an error",
    async run(t) {
      const p = await t.page();
      t.allow(p, /Failed to load resource/);
      await size(p, 1280);
      const pages = [
        ["list", t.url(""), `!!document.querySelector('[data-testid="list"] article[data-testid^="row-"]')`],
        ["dashboard", t.dash("nodes"), `!!document.querySelector('[data-testid="panel-1"]')`],
        ["not found", t.url("nothing/here"), `!!document.querySelector('[data-testid="not-found"]')`],
        ["dashboard not found", t.url("d/no-such-dashboard"), `!!document.querySelector('[data-testid="dashboard-not-found"]')`],
      ];
      let first = null;
      for (const [name, url, ready] of pages) {
        await p.goto(url);
        await p.idle();
        await p.waitFor(ready, { what: name });
        const got = await shape(p);
        if (!first) first = got;
        t.eq(got, first, `the selector on the page ${name}`);
        t.eq([got.w, got.h, got.count], [36, 32, 1], `size and number on ${name}`);
        const want = name === "list" || name === "not found" ? "light" : "dark";
        await choose(p, want);
        t.eq((await themeState(p)).theme, want, `a choice on ${name} changes the theme`);
      }
      // The editor: the dashboard in edit mode and the panel editor keep the selector.
      await p.goto(t.dash("nodes"));
      await p.idle();
      await waitPanelsReady(p, [1]);
      await enterEdit(p);
      t.eq(await shape(p), first, "the selector in edit mode");
      await p.click(p.tid("panel-1-menu"));
      await p.click(p.tid("panel-1-edit"));
      await p.waitSel(p.tid("panel-editor"));
      await p.idle();
      t.eq(await shape(p), first, "the selector in the panel editor");
      // A page that does not answer: the header and the selector are there before the session is.
      const q = await t.page();
      await size(q, 1280);
      t.allow(q, /Failed to load resource|ERR_/);
      q.failWhen((r) => /\/api\/session$/.test(r.url));
      await q.goto(t.url(""));
      await q.waitFor(`!!document.querySelector('[data-testid="no-answer"]')`, { what: "the page that does not answer" });
      t.eq(await shape(q), first, "the selector on the page of no answer");
    },
  },

  {
    id: "B51",
    name: "header bar: an editor finds the actions of the list and of a dashboard in the header, at 1280, 768 and 390 px",
    async run(t) {
      const p = await t.page();
      await size(p, 1280);
      await p.goto(t.url(""));
      await p.idle();
      await p.waitFor(`!!document.querySelector('[data-testid="list"] article[data-testid^="row-"]')`, { what: "the list" });
      const filterIds = ["list-sort", "list-tag-filter"];
      // One button New holds the actions (9.7): "New folder", "Import" and "New dashboard" are items of its menu, not buttons of the header.
      const actionIds = ["list-new-menu"];
      let w = await where(p, filterIds.concat(actionIds));
      t.eq([w.header, w.main, w.missing], [filterIds.concat(actionIds), [], []], "1280: every control of the list is in the header");
      t.eq((await rowInfo(p)).height, 44, "1280: the header is one row of 44 px with its line");
      t.eq((await rowInfo(p)).centres.length, 1, "1280: the controls are in one row " + JSON.stringify(await rowInfo(p)));
      t.eq(await p.eval(`["new-folder", "import", "new-dashboard"].filter((id) => document.querySelector('[data-testid="' + id + '"]'))`), [], "1280: the three old buttons are gone while the menu is closed");
      t.eq(await insideHeader(p, filterIds.concat(actionIds)), true, "1280: each control is in the box of the header");
      t.eq(await noOverflow(p), true, "1280: no horizontal scroll");
      // The sort and the tag filter follow the brand, at the left. The actions are at the right. The page has no search field and no view control.
      const order = await p.eval(`(() => { const x = (id) => document.querySelector('[data-testid="' + id + '"]').getBoundingClientRect(); const brand = document.querySelector(".brand").getBoundingClientRect(); return [brand.right <= x("list-sort").left, x("list-sort").right <= x("list-tag-filter").left, x("list-tag-filter").right <= x("list-new-menu").left]; })()`);
      t.eq(order, [true, true, true], "1280: the brand, the sort, the tag filter, then the actions");
      t.eq(await p.eval(`["list-search", "list-layout-folders", "list-layout-list"].filter((id) => document.querySelector('[data-testid="' + id + '"]'))`), [], "1280: no search field, no view control");
      // The controls work from the header: the tag filter opens its list, New dashboard opens its dialog.
      await p.click(p.tid("list-tag-filter"));
      await p.waitSel(p.tid("list-tag-menu"));
      await p.press("Escape");
      await p.click(p.tid("list-new-menu"));
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="list-new-menu-popup"] [role="menuitem"]')].map((e) => e.textContent.trim())`), ["New dashboard", "New folder", "Import"], "1280: the menu New has the three actions");
      await p.click(p.tid("new-dashboard"));
      await p.waitSel(p.tid("new-dashboard-dialog"));
      await p.press("Escape");

      await size(p, 768, 1024);
      w = await where(p, filterIds.concat(["list-new-menu"]));
      t.eq([w.header, w.main, w.missing], [filterIds.concat(["list-new-menu"]), [], []], "768: the filters and the button New are in the header");
      t.eq(await noOverflow(p), true, "768: no horizontal scroll");
      await size(p, 390, 844);
      w = await where(p, filterIds.concat(["list-new-menu"]));
      t.eq([w.header, w.main, w.missing], [filterIds.concat(["list-new-menu"]), [], []], "390: nothing of the list is in the body");
      t.eq((await rowInfo(p)).height, 44, "390: the header stays one row of 44 px");
      t.eq((await rowInfo(p)).centres.length, 1, "390: the controls are in one row " + JSON.stringify(await rowInfo(p)));
      t.eq(await insideHeader(p, filterIds.concat(["list-new-menu"])), true, "390: each control is in the box of the header");
      t.eq(await noOverflow(p), true, "390: no horizontal scroll");
      await p.click(p.tid("list-new-menu"));
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="list-new-menu-popup"] [role="menuitem"]')].map((e) => e.textContent.trim())`), ["New dashboard", "New folder", "Import"], "390: the menu New");
      await p.press("Escape");

      // A dashboard: the groups of buttons are in the header, at each width.
      const dashIds = ["time-picker", "refresh-picker", "edit-toggle", "dashboard-menu"];
      for (const [width, height] of [[1920, 1000], [1280, 900], [768, 1024], [390, 844]]) {
        await size(p, width, height);
        await p.goto(t.dash("nodes"));
        await p.idle();
        await waitPanelsReady(p, [1]);
        w = await where(p, dashIds);
        t.eq([w.header, w.main, w.missing], [dashIds, [], []], `${width}: the controls of the dashboard are in the header`);
        t.eq(await insideHeader(p, dashIds), true, `${width}: each control is in the box of the header`);
        t.eq(await noOverflow(p), true, `${width}: no horizontal scroll`);
      }
      // The names stay (the labels hide up to 768 px but stay for assistive technology). The menu works from the header.
      t.eq(await p.eval(`["edit-toggle", "dashboard-menu"].map((id) => document.querySelector('[data-testid="' + id + '"]').textContent.trim())`), ["Edit", "More actions"], "the names of the buttons");
      await p.click(p.tid("dashboard-menu"));
      t.eq(await p.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent.trim())`), ["Shift the time range back", "Shift the time range forward", "Zoom out", "History", "Export", "Delete"], "390: the menu More actions of an editor holds the time shift and the zoom out");
      await p.press("Escape");
      await size(p, 1280, 900);
      await p.click(p.tid("dashboard-menu"));
      t.eq(await p.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent.trim())`), ["History", "Export", "Delete"], "1280: the menu More actions of an editor");
      await p.press("Escape");
    },
  },

  {
    id: "B52",
    name: "header bar: a viewer (editing off) has the filters of the list and the buttons of a dashboard, and no edit control, in the header; the menu has History and Export",
    server: "noedit",
    async run(t) {
      const p = await t.page();
      for (const [width, height] of [[1280, 900], [390, 844]]) {
        await size(p, width, height);
        await p.goto(t.url(""));
        await p.idle();
        await p.waitFor(`!!document.querySelector('[data-testid="list"] article[data-testid^="row-"]')`, { what: "the list" });
        const kept = ["list-sort", "list-tag-filter"];
        const w = await where(p, kept);
        t.eq([w.header, w.main, w.missing], [kept, [], []], `${width}: the filters of the list are in the header`);
        t.eq((await where(p, ["new-folder", "import", "new-dashboard"])).missing, ["new-folder", "import", "new-dashboard"], `${width}: no action of an editor`);
        t.eq(await p.eval(`document.querySelector('[data-testid="list-new-menu"]').hidden`), true, `${width}: the button New stays hidden: it has no item`);
        t.eq(await noOverflow(p), true, `${width}: no horizontal scroll`);
        await p.goto(t.dash("nodes"));
        await p.idle();
        await waitPanelsReady(p, [1]);
        const ids = ["time-picker", "refresh-picker", "dashboard-menu"];
        const d = await where(p, ids.concat(["edit-toggle"]));
        t.eq([d.header, d.main, d.missing], [ids, [], ["edit-toggle"]], `${width}: the buttons of a dashboard, with no Edit`);
        t.eq(await insideHeader(p, ids), true, `${width}: each control is in the box of the header`);
        await p.click(p.tid("dashboard-menu"));
        t.eq(await p.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent.trim()).filter((x) => !/time range|Zoom/.test(x))`), ["History", "Export"], `${width}: the menu More actions has History and Export only`);
        await p.press("Escape");
        t.eq(await noOverflow(p), true, `${width}: no horizontal scroll on a dashboard`);
      }
    },
  },

  {
    id: "B53",
    name: "version line: `version:checksum` beside the product name above 1280 px, the same on every page, with a title that names the source",
    async run(t) {
      const sess = (await t.api.session()).json;
      const p = await t.page();
      t.allow(p, /Failed to load resource/);
      await size(p, 1440);
      const line = (page) =>
        page.eval(`(() => {
          const v = document.querySelector('[data-testid="app-version"]');
          const name = document.querySelector(".brand__name");
          const logo = document.querySelector(".brand .icon");
          const link = document.querySelector("a.brand");
          const vb = v.getBoundingClientRect();
          const nb = name.getBoundingClientRect();
          return {
            text: v.textContent, title: v.getAttribute("title"), linkTitle: link.getAttribute("title"), beside: vb.left >= nb.right - 1 && Math.abs(vb.top + vb.height / 2 - (nb.top + nb.height / 2)) <= 8 && vb.height <= 18,
            visible: vb.width > 0 && getComputedStyle(v).visibility === "visible", cut: v.scrollWidth > v.clientWidth,
            describedBy: link.getAttribute("aria-describedby") === v.id, name: link.getAttribute("aria-label"), logoLeft: logo.getBoundingClientRect().right <= nb.left,
            boot: JSON.parse(document.getElementById("hune-boot").textContent),
          };
        })()`);
      await p.goto(t.url(""));
      await p.idle();
      const home = await line(p);
      t.assert(VERSION_RE.test(home.text), "the format version:checksum: " + home.text);
      const [version, checksum] = home.text.split(":");
      t.eq(version, sess.version, "the version is the version of GET api/session");
      t.eq(checksum.length, 9, "nine hexadecimal characters");
      t.assert(/^Hune \S+, (commit [0-9a-f]{9,}|build [0-9a-f]{9,} \(the hash of the web files; this build has no commit\))$/.test(home.title), "the title gives the full value and the source: " + home.title);
      const source = /commit ([0-9a-f]+)/.exec(home.title) || /build ([0-9a-f]+)/.exec(home.title);
      t.assert(source[1].startsWith(checksum), "the checksum is the start of the value of the title");
      if (/build /.test(home.title)) t.eq(source[1], sess.build, "a build without commit: the value is the build hash of GET api/session");
      t.eq([home.beside, home.visible, home.cut, home.linkTitle], [true, true, false, home.title], "beside the name on one line, visible, not cut; the link has the title");
      t.eq([home.describedBy, home.name, home.logoLeft], [true, "Hune, the list of dashboards", true], "the link keeps its name; the line describes it");
      t.eq(home.boot.version, home.text, "the page reads the line from the boot block of the shell");
      t.assert(!/[\\/]/.test(home.text) && !/[\\/]/.test(home.title), "no file path in the text");
      // The same text and title on the other pages.
      for (const [name, url, ready] of [
        ["dashboard", t.dash("nodes"), `!!document.querySelector('[data-testid="panel-1"]')`],
        ["not found", t.url("nothing/here"), `!!document.querySelector('[data-testid="not-found"]')`],
        ["dashboard not found", t.url("d/no-such-dashboard"), `!!document.querySelector('[data-testid="dashboard-not-found"]')`],
      ]) {
        await p.goto(url);
        await p.idle();
        await p.waitFor(ready, { what: name });
        const other = await line(p);
        t.eq([other.text, other.title, other.visible, other.beside], [home.text, home.title, true, true], `the line on the page ${name}`);
      }
      // The page that does not answer (no session) still shows it: the value does not come from the API.
      const q = await t.page();
      await size(q, 1280);
      t.allow(q, /Failed to load resource|ERR_/);
      q.failWhen((r) => /\/api\/session$/.test(r.url));
      await q.goto(t.url(""));
      await q.waitFor(`!!document.querySelector('[data-testid="no-answer"]')`, { what: "the page of no answer" });
      t.eq(await q.eval(`document.querySelector('[data-testid="app-version"]').textContent`), home.text, "the line on the page of no answer");
      // Up to 1280 px the line is for assistive technology only. The title of the link still gives the value. At 390 px the name hides too.
      await size(p, 1280);
      await p.goto(t.url(""));
      await p.idle();
      t.eq(await p.eval(`(() => { const v = document.querySelector('[data-testid="app-version"]'); return [v.getBoundingClientRect().width <= 1, v.textContent, document.querySelector("a.brand").title]; })()`), [true, home.text, home.title], "1280: the line is for assistive technology; the tooltip of the brand has it");
      await size(p, 390, 844);
      await p.goto(t.url(""));
      await p.idle();
      t.eq(await p.eval(`(() => { const v = document.querySelector('[data-testid="app-version"]'); return [v.getBoundingClientRect().width <= 1, v.textContent]; })()`), [true, home.text], "390: the line is for assistive technology only");
    },
  },

  {
    id: "B54",
    name: "header bar: the edit mode adds Add, Save and Revert to the same row, the switch Edit turns on, the views are in the menu More actions, and the header stays one row of 44 px",
    async run(t) {
      const p = await t.page();
      await size(p, 1920, 1000);
      // No time range in the URL: the range button then has the short text of the dashboard range (an absolute range is wider).
      await p.goto(t.url("d/nodes"));
      await p.idle();
      await p.waitSel(p.tid("panel-1"));
      t.eq(await p.attr(p.tid("edit-toggle"), "aria-checked"), "false", "the switch is off in the view");
      await enterEdit(p);
      t.eq(await p.attr(p.tid("edit-toggle"), "aria-checked"), "true", "the switch is on in edit mode");
      const editIds = ["edit-toggle", "add-button", "add-menu", "save-button", "discard-button", "time-picker", "refresh-picker", "dashboard-menu"];
      let w = await where(p, editIds);
      t.eq([w.header, w.main, w.missing], [editIds, [], []], "1920: Add, Save and Revert are in the header");
      t.eq((await rowInfo(p)).height, 44, "1920: the header stays 44 px " + JSON.stringify(await rowInfo(p)));
      t.eq((await rowInfo(p)).centres.length, 1, "1920: one row " + JSON.stringify(await rowInfo(p)));
      t.eq(await p.eval(`["add-panel", "add-section", "add-variable", "json-tab", "dash-settings", "exit-button"].filter((id) => document.querySelector('[data-testid="' + id + '"]'))`), [], "no button of its own for the items of the menus");
      t.eq(await insideHeader(p, editIds), true, "1920: each control is in the box of the header");
      for (const width of [1280, 768, 390]) {
        await size(p, width, width === 390 ? 844 : 900);
        w = await where(p, editIds);
        t.eq([w.header, w.main, w.missing], [editIds, [], []], `${width}: the controls of the edit mode are in the header`);
        t.eq((await rowInfo(p)).centres.length, 1, `${width}: one row ` + JSON.stringify(await rowInfo(p)));
        t.eq((await rowInfo(p)).height, 44, `${width}: 44 px`);
        // Up to 560 px "Revert" is in the menu More actions: its button does not show.
        t.eq(await insideHeader(p, width <= 560 ? editIds.filter((id) => id !== "discard-button") : editIds), true, `${width}: each control is in the box of the header`);
        t.eq(await noOverflow(p), true, `${width}: no horizontal scroll`);
      }
      await size(p, 1280);
      // The menu has the views of the edit mode, then History, Export and Delete. Add panel and Add section are in the menu of Add.
      await p.click(p.tid("dashboard-menu"));
      t.eq(await p.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent.trim())`), ["Dashboard JSON", "Settings", "History", "Export", "Delete"], "the menu More actions in edit mode");
      await p.press("Escape");
      await p.click(p.tid("add-menu"));
      t.eq(await p.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent.trim())`), ["Add panel", "Add text panel", "Add section", "Place a variable in the grid"], "the menu of Add");
      await p.press("Escape");
      // The panel editor keeps the whole header: the switch stays on, and the menu has no action for the draft grid.
      await p.click(p.tid("panel-1-menu"));
      await p.click(p.tid("panel-1-edit"));
      await p.waitSel(p.tid("panel-editor"));
      await p.idle();
      const shown = await p.eval(`["time-picker", "refresh-picker", "edit-toggle", "add-button", "add-menu", "save-button", "discard-button", "dashboard-menu"].filter((id) => { const e = document.querySelector('[data-testid="' + id + '"]'); return e && e.offsetParent !== null; })`);
      t.eq(shown, ["time-picker", "refresh-picker", "edit-toggle", "add-button", "add-menu", "save-button", "discard-button", "dashboard-menu"], "the panel editor: the whole header");
      t.eq(await p.eval(`[document.querySelector('[data-testid="add-button"]').disabled, document.querySelector('[data-testid="add-menu"]').disabled]`), [true, true], "Add keeps its place in the panel editor and is not active");
      t.eq(await p.attr(p.tid("edit-toggle"), "aria-checked"), "true", "the switch is on in the panel editor");
      t.eq((await where(p, ["time-picker", "refresh-picker"])).header, ["time-picker", "refresh-picker"], "they are in the header");
      await p.click(p.tid("dashboard-menu"));
      t.eq(await p.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent.trim())`), ["Dashboard JSON", "Settings", "History", "Export", "Delete"], "the menu in the panel editor");
      await p.press("Escape");
    },
  },
];
