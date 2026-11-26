// SPDX-License-Identifier: MIT OR Apache-2.0
// Tests of the grouping, the tag filter, the short age, the threshold of the filters and the rights of a card on the list page (9.7).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { cardRights, filtersShown, groupDashboards, lastChange, matchesTag, shortAge, tagColor } from "../../web/js/list.js";
import { switcherItems } from "../../web/js/switcher.js";

const read = (p) => readFileSync(new URL(`../../web/${p}`, import.meta.url), "utf8");
const folders = [{ id: "b", title: "beta" }, { id: "a", title: "Alpha" }, { id: "c", title: "Zeta" }];
const item = (uid, folder, title, tags = []) => ({ uid, folder, title, tags });

test("groups in folder title order and cards in title order, ASCII fold", () => {
  const groups = groupDashboards(folders, [item("1", "b", "x"), item("2", "a", "Zed"), item("3", "a", "alpha"), item("4", "a", "Beta")]);
  assert.deepEqual(groups.map((g) => g.folder.id), ["a", "b", "c"]);
  assert.deepEqual(groups[0].items.map((i) => i.uid), ["3", "4", "2"]);
  assert.equal(groups[2].items.length, 0);
});

test("the tag filter matches one tag name, with ASCII letters folded, and not a part of a name, a title or a uid", () => {
  const i = item("Abc-1", "a", "Weight Report", ["Chain", "ops"]);
  assert.ok(matchesTag(i, "chain"));
  assert.ok(matchesTag(i, "CHAIN"));
  assert.ok(matchesTag(i, "ops"));
  assert.ok(!matchesTag(i, "chai"), "a part of a tag");
  assert.ok(!matchesTag(i, "weight"), "a word of the title");
  assert.ok(!matchesTag(i, "abc"), "a part of the uid");
  assert.ok(matchesTag(i, ""), "no filter");
  assert.ok(!matchesTag(item("u", "a", "t"), "chain"), "an item with no tag");
});

test("the Z-A preference reverses the folders and the dashboards", () => {
  const groups = groupDashboards(folders, [item("1", "b", "x"), item("2", "a", "Zed"), item("3", "a", "alpha")], { descending: true });
  assert.deepEqual(groups.map((g) => g.folder.id), ["c", "b", "a"]);
  assert.deepEqual(groups[2].items.map((i) => i.uid), ["2", "3"]);
});

test("a tag has the colour that Grafana 12.4.2 gives it, whatever the case", () => {
  const measured = { demo: "#3f2b5b", ops: "#3f2b5b", density: "#4c7a3f", overview: "#584477", production: "#052b51", showcase: "#ae561a", states: "#890f02", panels: "#d32d20", tables: "#000000", variables: "#3d71aa", stream: "#99440a" };
  for (const [name, color] of Object.entries(measured)) assert.equal(tagColor(name), color, name);
  assert.equal(tagColor("Showcase"), tagColor("showcase"));
});

test("the short age: now, minutes, hours, days, weeks, months and years, rounded down", () => {
  const now = Date.parse("2026-11-24T12:00:00Z");
  const ago = (ms) => new Date(now - ms).toISOString();
  const MIN = 60000;
  const HOUR = 60 * MIN;
  const DAY = 24 * HOUR;
  assert.equal(shortAge(ago(0), now), "now");
  assert.equal(shortAge(ago(59 * 1000), now), "now");
  assert.equal(shortAge(ago(-5 * MIN), now), "now", "a time in the future");
  assert.equal(shortAge(ago(MIN), now), "1m");
  assert.equal(shortAge(ago(59 * MIN), now), "59m");
  assert.equal(shortAge(ago(HOUR), now), "1h");
  assert.equal(shortAge(ago(2 * HOUR + 59 * MIN), now), "2h");
  assert.equal(shortAge(ago(23 * HOUR + 59 * MIN), now), "23h");
  assert.equal(shortAge(ago(DAY), now), "1d");
  assert.equal(shortAge(ago(3 * DAY), now), "3d");
  assert.equal(shortAge(ago(6 * DAY + 23 * HOUR), now), "6d");
  assert.equal(shortAge(ago(7 * DAY), now), "1w");
  assert.equal(shortAge(ago(14 * DAY), now), "2w");
  assert.equal(shortAge(ago(29 * DAY), now), "4w");
  assert.equal(shortAge(ago(30 * DAY), now), "1mo");
  assert.equal(shortAge(ago(364 * DAY), now), "12mo");
  assert.equal(shortAge(ago(365 * DAY), now), "1y");
  assert.equal(shortAge("not a time", now), "");
  assert.equal(shortAge(undefined, now), "");
});

test("the filters show only above the threshold of the session; a missing or invalid member shows them", () => {
  const at = (min, count) => filtersShown({ list_filters_min_dashboards: min }, count);
  assert.equal(at(6, 6), false, "equal: hidden");
  assert.equal(at(6, 7), true);
  assert.equal(at(6, 1), false);
  assert.equal(at(0, 1), true, "0 shows them for any list that is not empty");
  assert.equal(at(0, 0), false);
  assert.equal(at(1000, 1000), false);
  for (const bad of [undefined, null, -1, 1.5, "6", NaN, Infinity, 2 ** 60]) assert.equal(at(bad, 3), true, `invalid member ${String(bad)} shows the filters`);
  assert.equal(filtersShown({}, 3), true);
  assert.equal(filtersShown(null, 3), true);
  assert.equal(filtersShown(undefined, 0), true);
});

test("a card has its buttons only with editing on and the write right; the pencil and the bin share the condition", () => {
  assert.deepEqual(cardRights({ access: { write: true } }, true), { write: true, export: false });
  assert.deepEqual(cardRights({ access: { write: false } }, true), { write: false, export: false });
  assert.deepEqual(cardRights({ access: { write: true } }, false), { write: false, export: false }, "editing off");
  assert.deepEqual(cardRights({ access: {} }, true), { write: false, export: false });
  assert.deepEqual(cardRights({}, true), { write: false, export: false });
  // The flag `access.export` is its own condition (6.12): it does not depend on the write right or on editing.
  assert.deepEqual(cardRights({ access: { write: false, export: true } }, true), { write: false, export: true });
  assert.deepEqual(cardRights({ access: { export: true } }, false), { write: false, export: true });
  assert.deepEqual(cardRights({ access: { export: "true" } }, true), { write: false, export: false }, "a text is not true");
  assert.deepEqual(cardRights({ access: { write: true } }, undefined), { write: false, export: false });
  assert.deepEqual(cardRights({ access: { write: true } }, "true"), { write: false, export: false }, "a text is not true");
});

test("the page has no search field, no view control, no collapse and no table; it keeps the sort in one key", () => {
  const js = read("js/list.js");
  for (const gone of ["hune.list.collapsed", "hune.list.layout", "list-search", "list-layout", "readCollapsed", "matchesQuery", "<table", '"table"', "list-colhead"]) {
    assert.ok(!js.includes(gone), `${gone} is gone from list.js`);
  }
  assert.match(js, /const SORT_KEY = "hune\.list\.sort"/);
  assert.ok(!/localStorage/.test(js.replace(/function (readPref|writePref)[\s\S]*?\n}\n/g, "")), "the sort is the only stored choice");
  // The page keeps no copy of the threshold: it reads the session at each paint.
  assert.ok(!/list_filters_min_dashboards\s*[=:]/.test(js.replace(/session\.list_filters_min_dashboards/g, "")), "no copy of the number");
});

test("the card is an article with a stretched title link, and its buttons are siblings of the link", () => {
  const js = read("js/list.js");
  assert.match(js, /el\("article", \{ class: "card"/);
  assert.match(js, /class: "card__link"/);
  assert.match(js, /\?edit=1/);
  assert.match(js, /callEditor\("folders", "deleteDashboard", item\)/);
  assert.match(js, /callEditor\("folders", "moveDashboard", item\)/);
  const css = read("css/list.css");
  assert.match(css, /\.card__link::after \{ content: ""; position: absolute; inset: 0;/);
  assert.match(css, /\.card__more \{ position: relative; z-index: var\(--z-raised\);/);
  assert.match(css, /\.list-stack \{ display: grid; grid-template-columns: repeat\(auto-fit, 400px\);/);
  assert.match(css, /\.group, \.cards \{ display: contents; \}/);
  assert.match(css, /\.group__head \{\s*grid-column: 1 \/ -1;/);
});

// The badge of the last change (9.7): only a change that Hune recorded, and only where the system allows editing.
test("the badge of the last change shows only for a change that Hune recorded in a system that allows editing", () => {
  const now = Date.parse("2026-11-25T12:00:00Z");
  const saved = { version: 2, updated_by: "user:ana", updated_at: "2026-11-24T12:00:00.000Z" };
  assert.equal(lastChange(saved, true, now).age, "1d");
  assert.match(lastChange(saved, true, now).title, /2026/);
  assert.equal(lastChange({ ...saved, updated_by: "-" }, true, now).age, "1d", "open mode: the author is unknown, the change is recorded");
  assert.equal(lastChange(saved, false, now), null, "a system that does not allow editing has no change");
  assert.equal(lastChange(saved, undefined, now), null);
  assert.equal(lastChange({ ...saved, version: 0 }, true, now), null, "no version in the history: the date is the date of a file");
  assert.equal(lastChange({ ...saved, updated_by: "external" }, true, now), null, "a change outside Hune is the date of a file");
  assert.equal(lastChange({ ...saved, updated_by: undefined }, true, now), null, "a list with no author gives no badge");
  assert.equal(lastChange({ ...saved, updated_at: "not a time" }, true, now), null);
  assert.equal(lastChange({ ...saved, version: 1.5 }, true, now), null);
});

test("the badge and the three dots are at the top right of the card, the dots after the badge", () => {
  const js = read("js/list.js");
  assert.match(js, /el\("div", \{ class: "card__top" \}, el\("h3", \{ class: "card__title" \}/);
  assert.match(js, /`Last change \$\{change\.age\}`/);
  assert.ok(!js.includes("card__updated"), "no note under the title");
  // The dots are appended to the head row after the title and the badge: the order is title, badge, dots.
  assert.match(js, /card\.querySelector\("\.card__top"\)\.append\(more\)/);
  const css = read("css/list.css");
  assert.match(css, /\.card__more \{[^}]*width: 24px;[^}]*height: 24px;/);
  assert.match(css, /\.card__badge \{[^}]*height: 20px/);
  for (const gone of [".card__buttons", ".card--actions", ".card__edit", ".card__delete"]) assert.ok(!css.includes(gone), `${gone} is gone from the css`);
  assert.ok(!js.includes("card__buttons") && !js.includes("card--actions") && !js.includes("card__edit") && !js.includes("card__delete"), "the pencil and the bin are gone from the card");
});

// The three dots are the one menu of a card (9.7): Edit, Export, Move and Delete for a card that the user can edit; Download and Replace for an
// entry that is not valid; no button for a read-only card. The condition is the one of cardRights.
test("the menu of a card has Edit, Export, Move and Delete, in this order, for the right to write only", () => {
  const js = read("js/list.js");
  const menu = js.slice(js.indexOf("let entries = null;"), js.indexOf("if (entries && entries.length) {"));
  const labels = [...menu.matchAll(/label: "([A-Za-z]+)"/g)].map((m) => m[1]);
  assert.deepEqual(labels, ["Edit", "Export", "Move", "Delete", "Download", "Replace"]);
  assert.match(menu, /if \(valid && rights\.write\)/, "a valid card has the four entries for the right to write only");
  assert.match(menu, /\} else if \(!valid\) \{/, "an entry that is not valid has its own entries");
  assert.match(menu, /if \(rights\.write\) entries\.push\(\{ label: "Replace"/, "Replace needs the right to write");
  assert.match(menu, /label: "Edit", icon: "edit", href: `\$\{dashboardPath\(item\.uid, item\.title\)\}\?edit=1`, testid: `card-edit-\$\{item\.uid\}`/);
  assert.match(menu, /label: "Delete", icon: "trash", danger: true, testid: `card-delete-\$\{item\.uid\}`/);
  assert.match(js, /`row-menu-\$\{item\.uid\}`/, "the button of the menu keeps its hook");
  // A read-only card gets no button: the button is made only when there are entries.
  assert.match(js, /if \(entries && entries\.length\) \{\s*const more = cardButton/);
  // Export and Download need `access.export`; the entry is not made without it.
  assert.match(js, /const exportOk = cardRights\(item, editing\)\.export;/);
  assert.match(menu, /\.\.\.\(exportOk \? \[\{ label: "Export"/);
  assert.match(menu, /if \(exportOk\) entries\.push\(\{ label: "Download"/);
  assert.equal(cardRights({ access: { write: true } }, true).write, true);
  assert.equal(cardRights({ access: { write: true } }, false).write, false, "editing off: no menu entry that changes the dashboard");
  assert.equal(cardRights({ access: { write: false } }, true).write, false);
  assert.equal(cardRights({}, true).write, false);
});

// The button of the sort shows the order that applies (7): A–Z or Z–A in short, never a placeholder. No saved choice is A–Z.
test("the button of the sort shows A–Z or Z–A and the menu marks the order that applies", () => {
  const js = read("js/list.js");
  assert.match(js, /const SORT_SHORT = \{ az: "A–Z", za: "Z–A" \};/);
  assert.match(js, /sortLabel\.textContent = SORT_SHORT\[sort === "za" \? "za" : "az"\];/);
  assert.ok(!/sortLabel\.textContent = sort \? /.test(js) && !js.includes('"Sort" :'), "no placeholder text in the button");
  assert.ok(!/sortLabel\.classList\.toggle\("selector__text--placeholder"/.test(js), "the sort text is never muted as a placeholder");
  assert.match(js, /label: SORT_LABELS\.az, checked: sort !== "za"/, "no saved choice marks A–Z");
  assert.match(js, /label: SORT_LABELS\.za, checked: sort === "za"/);
});

// The selector of the header (9.6): folders as rows with their dashboards, one folder as a flat list.
test("the selector lists the folders as groups, opens the folder of the current dashboard and keeps the choice of the person", () => {
  const data = {
    folders: [{ id: "ops", title: "Operations" }, { id: "inf", title: "Infrastructure" }, { id: "none", title: "Empty" }],
    items: [
      { uid: "traffic", folder: "ops", title: "Traffic", state: "valid" },
      { uid: "latency", folder: "ops", title: "Latency", state: "valid" },
      { uid: "servers", folder: "inf", title: "Servers", state: "valid" },
      { uid: "broken", folder: "inf", title: "broken", state: "invalid" },
    ],
  };
  const go = [];
  const items = switcherItems(data, "traffic", (href) => go.push(href));
  assert.deepEqual(items.map((i) => i.label), ["Infrastructure", "Operations"], "folders in title order, an empty folder is not listed");
  assert.deepEqual(items.map((i) => i.open), [false, true], "the folder of the current dashboard is open");
  assert.deepEqual(items[1].children.map((c) => c.label), ["Latency", "Traffic"]);
  assert.deepEqual(items[1].children.map((c) => c.current), [false, true]);
  assert.deepEqual(items[0].children.map((c) => c.key), ["servers"], "an entry that is not valid is not in the menu");
  assert.match(items[1].children[1].href, /traffic/);
  items[1].children[0].onSelect();
  assert.match(go[0], /latency/);
  // The person closes the open folder and opens the other one: the next menu keeps both choices.
  items[1].onToggle(false);
  items[0].onToggle(true);
  const again = switcherItems(data, "traffic", () => {});
  assert.deepEqual(again.map((i) => i.open), [true, false]);
});

test("the selector lists the dashboards alone when the user sees one folder", () => {
  const data = { folders: [{ id: "ops", title: "Operations" }, { id: "x", title: "Other" }], items: [{ uid: "a", folder: "ops", title: "B title", state: "valid" }, { uid: "b", folder: "ops", title: "A title", state: "valid" }] };
  const items = switcherItems(data, "a", () => {});
  assert.deepEqual(items.map((i) => i.label), ["A title", "B title"]);
  assert.ok(items.every((i) => !i.group));
  assert.deepEqual(items.map((i) => i.current), [false, true]);
  assert.deepEqual(switcherItems({ folders: [], items: [] }, "a", () => []), []);
});

test("the three dropdowns use the one menu of the app, joined to their button", () => {
  const list = read("js/list.js");
  assert.match(list, /bindMenu\(tagButton, tagItems, \{ joined: true, testid: "list-tag-menu" \}\)/);
  assert.match(list, /\], \{ joined: true, testid: "list-sort-menu" \}\)/);
  assert.ok(!/anchorPopup|list-tagmenu|onTagKey|openTagMenu/.test(list), "the popup of the tag filter is gone");
  // The menu of the selector has the width of its button exactly (fit): a long label ends with an ellipsis.
  assert.match(read("js/switcher.js"), /bindMenu\(trigger, items, \{ joined: true, fit: true, testid: MENU_TESTID \}\)/);
  const ui = read("js/ui.js");
  assert.match(ui, /joined = false, fit = false \} = \{\}\)/);
  assert.match(ui, /const FIT_MIN_WIDTH = 224;/);
  assert.match(ui, /if \(joined && fit\) popup\.style\.width = popup\.style\.maxWidth = `\$\{Math\.min\(Math\.max\(Math\.round\(box\.width\), FIT_MIN_WIDTH\), window\.innerWidth - 8\)\}px`;/);
  assert.match(read("css/base.css"), /\.location \.switcher \{ flex: 0 1 22rem; min-width: 0; max-width: 22rem; \}/);
  for (const part of ["export function bindMenu", "popup.update = ", "A typed prefix jumps to the next item", 'event.key === "ArrowRight" && group', "is-joined-open"]) assert.ok(ui.includes(part), part);
});

// One button "New" holds the actions of the list (9.7): the same button in the header and in the empty state, at every width.
test("one button New holds New dashboard, New folder and Import, in this order, each for its own condition", () => {
  const js = read("js/list.js");
  const entries = js.slice(js.indexOf("function newEntries()"), js.indexOf("function makeNewButton"));
  const labels = [...entries.matchAll(/label: "([A-Za-z ]+)"/g)].map((m) => m[1]);
  assert.deepEqual(labels, ["New dashboard", "New folder", "Import"]);
  assert.deepEqual([...entries.matchAll(/testid: "([a-z-]+)"/g)].map((m) => m[1]), ["new-dashboard", "new-folder", "import"]);
  assert.match(entries, /if \(canWrite\(\)\) entries\.push\(\{ label: "New dashboard"/, "new dashboard needs the right to write in a folder");
  assert.match(entries, /if \(editing && folderAccess\.create\) entries\.push\(\{ label: "New folder"/, "new folder needs editing and access.create");
  assert.match(entries, /if \(canWrite\(\)\) entries\.push\(\{ label: "Import"/);
  assert.match(js, /const newMenu = makeNewButton\("list-new-menu"\);/);
  assert.equal([...js.matchAll(/makeNewButton\("new-menu-empty"\)/g)].length, 2, "the empty state shows the same button in its two forms");
  assert.match(js, /newMenu\.hidden = !items\.length \|\| newEntries\(\)\.length === 0;/, "no item, no button");
  assert.match(js, /bindMenu\(node, newEntries, \{ align: "end", gap: 4, testid: "list-new-menu-popup" \}\)/);
  // The three buttons, their box and the rule of 900 px are gone.
  for (const gone of ["actionsBox", "data-toolbar-action", "toolbarAction", "list-actions"]) assert.ok(!js.includes(gone), `${gone} is gone from list.js`);
  const css = read("css/list.css");
  assert.ok(!css.includes(".list-actions") && !/max-width: 900px/.test(css), "no box for the three buttons and no rule of 900 px in the list css");
  assert.match(read("css/look.css"), /\.selector, \.list-new \{ border-radius: var\(--r-control\); \}/);
});

test("the tag chips are more vivid: the shares of the tag colour", () => {
  const look = read("css/look.css");
  assert.match(look, /--tag-fill-share: 32%;/);
  assert.match(look, /--tag-edge-share: 65%;/);
  // The ink of the light theme is 70 % of the colour (black at 30 %), the ink of the dark theme is 40 % of the colour (white at 60 %).
  assert.equal([...look.matchAll(/--tag-ink-share: 70%;/g)].length, 2, "light: in the media query and in the attribute");
  assert.equal([...look.matchAll(/--tag-ink-share: 40%;/g)].length, 2, "dark: in the root and in the attribute");
});
