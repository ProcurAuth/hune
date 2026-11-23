// The dashboard links (9.8a, F19, FE-25): the URLs, the items of a `dashboards` link, and the group in the toolbar row.
// The functions before `createLinksBar` are pure. The group builds its nodes with textContent and property setters only.

import { allVariables, checkUrl, expandUrl } from "./links.js";
import { el, icon, openMenu } from "./ui.js";
import { dashboardPath } from "./url_state.js";

/** The most items that one `dashboards` link shows (9.8a, step 4). */
export const MAX_ITEMS = 50;
/** The most pages of the list that the page reads for one link (9.8a, step 1). */
export const MAX_PAGES = 3;
/** The size of one page of the list (6.24). */
export const PAGE_LIMIT = 500;

const enc = encodeURIComponent;

/** The text of the added names (9.8a): the time first, then the variables. `ctx` has `fromText`, `toText` and `variables`. */
export function addedNames(link, ctx) {
  const parts = [];
  if (link.keep_time === true) parts.push(`from=${enc(ctx.fromText)}&to=${enc(ctx.toText)}`);
  if (link.include_vars === true) {
    const vars = allVariables(ctx.variables);
    if (vars !== "") parts.push(vars);
  }
  return parts.join("&");
}

/**
 * Puts `names` at the end of the query of a URL (9.8a). It writes `?` before them when the URL has no `?`, else `&`. A fragment that
 * starts with `#` stays at the end: the names go before it (Grafana puts them after it, known difference).
 */
export function appendNames(url, names) {
  if (names === "") return url;
  const hash = url.indexOf("#");
  const base = hash < 0 ? url : url.slice(0, hash);
  const fragment = hash < 0 ? "" : url.slice(hash);
  return `${base}${base.includes("?") ? "&" : "?"}${names}${fragment}`;
}

/**
 * The final URL of a link of the kind `link` (9.8a), or null when the check of 4.23 fails. Seven tokens of `url` change: `$<name>`,
 * `${<name>}`, `${<name>:queryparam}`, `${__from}`, `${__to}`, `${__url_time_range}` and `${__all_variables}`. Another token stays.
 * `ctx`: { from, to (epoch ms), fromText, toText, variables: [{ name, kind, all, values }] }.
 */
export function linkHref(link, ctx) {
  if (!link || link.kind !== "link" || typeof link.url !== "string") return null;
  // The context of the data links has more members (`__value.raw` and the like): this call gives only what the seven tokens need.
  const tokens = { from: ctx.from, to: ctx.to, variables: ctx.variables };
  const url = appendNames(expandUrl(link.url, tokens), addedNames(link, ctx));
  return checkUrl(url) ? url : null;
}

/** The URL of one dashboard of a `dashboards` link: `d/<uid>/<slug>` and the added names (9.8a). It is relative to the base path. */
export function itemHref(link, item, ctx) {
  return appendNames(dashboardPath(item.uid, item.title), addedNames(link, ctx));
}

/** The tag that the page sends to the list route (6.12): the first tag of the link, or null for a link with no tag. */
export function listTag(link) {
  return Array.isArray(link.tags) && link.tags.length ? link.tags[0] : null;
}

/** Order of two items: the title in lower case by UTF-16 code unit, then the uid (9.8a, step 4). */
function compareItems(a, b) {
  const x = a.title.toLowerCase();
  const y = b.title.toLowerCase();
  if (x !== y) return x < y ? -1 : 1;
  return a.uid < b.uid ? -1 : a.uid > b.uid ? 1 : 0;
}

/**
 * The items of a `dashboards` link (9.8a, steps 2 to 4): the valid items that have all the tags of the link (exact comparison), not
 * the current dashboard, sorted, 50 at most. `items` is the list of the route. The result is a new list.
 */
export function dashboardItems(link, items, currentUid) {
  const tags = Array.isArray(link.tags) ? link.tags : [];
  return (items || [])
    .filter((item) => item && item.state === "valid" && typeof item.uid === "string" && typeof item.title === "string" && item.uid !== currentUid
      && tags.every((tag) => Array.isArray(item.tags) && item.tags.includes(tag)))
    .sort(compareItems)
    .slice(0, MAX_ITEMS);
}

/**
 * Reads the list for a `dashboards` link (9.8a, step 1): `list({ tag, limit, after })` gives one page `{ items, next }`. The function
 * follows `next` for MAX_PAGES pages at most. It returns the items of the link, or [] when a request fails (the caller writes the warning).
 */
export async function fetchItems(link, list, currentUid) {
  const tag = listTag(link);
  const all = [];
  let after = null;
  for (let page = 0; page < MAX_PAGES; page += 1) {
    const answer = await list({ tag, limit: PAGE_LIMIT, after });
    all.push(...(answer.items || []));
    if (!answer.next) break;
    after = answer.next;
  }
  return dashboardItems(link, all, currentUid);
}

// --- the group in the toolbar row -------------------------------------------------------------------------------------------

/**
 * The group `dash-links` of the toolbar row (9.8a).
 * o: { links, uid, ctx(): the context of linkHref, interpolate(text): text, loadItems(link): Promise<items> }
 * It returns { el, update(), destroy() }. `update` writes the nodes again after a change of the range or of a variable.
 */
export function createLinksBar({ links, ctx, interpolate, loadItems }) {
  const root = el("div", { class: "dash-links", dataset: { testid: "dash-links" } });
  const items = new Map(); // link index -> the items of a `dashboards` link, when they are read
  const warned = new Set();
  let destroyed = false;

  const warn = (key, text) => {
    if (warned.has(key)) return;
    warned.add(key);
    console.warn(text);
  };

  function anchor(testid, href, blank, noopener) {
    const node = el("a", { class: "btn btn--tb dash-link", dataset: { testid } });
    node.setAttribute("href", href);
    if (blank) {
      node.target = "_blank";
      node.rel = noopener ? "noopener noreferrer" : "noreferrer";
    }
    return node;
  }

  function paint() {
    root.replaceChildren();
    const c = ctx();
    links.forEach((link, n) => {
      const title = interpolate(link.title);
      if (link.kind === "link") {
        const href = linkHref(link, c);
        if (href === null) {
          warn(`${n}`, `The dashboard link ${n} has a URL that Hune does not permit. The page does not show it.`);
          return;
        }
        const node = anchor(`dash-link-${n}`, href, link.target_blank === true, false);
        node.append(icon("external", 16), el("span", { class: "dash-link__text", text: title }));
        root.append(node);
        return;
      }
      const list = items.get(n);
      if (list === undefined) return;
      if (!list.length) return; // A link with no dashboard shows nothing, and a dropdown with no dashboard shows no button (9.8a).
      if (link.as_dropdown === true) {
        const group = el("div", { class: "dash-link-group", dataset: { testid: `dash-link-${n}` } });
        const open = el("button", { type: "button", class: "btn btn--tb dash-link__button", "aria-haspopup": "menu", "aria-expanded": "false", dataset: { testid: `dash-link-${n}-button` } },
          el("span", { class: "dash-link__text", text: title }), icon("chevron-down", 16));
        open.addEventListener("click", () => {
          const now = ctx();
          openMenu(open, list.map((item) => ({ label: item.title, href: itemHref(link, item, now), blank: link.target_blank === true, testid: `dash-link-${n}-item-${item.uid}` })), { align: "start", gap: 4 });
        });
        group.append(open);
        root.append(group);
      } else {
        const group = el("div", { class: "dash-link-group", dataset: { testid: `dash-link-${n}` } });
        for (const item of list) {
          const node = anchor(`dash-link-${n}-item-${item.uid}`, itemHref(link, item, c), link.target_blank === true, false);
          node.append(el("span", { class: "dash-link__text", text: item.title }));
          group.append(node);
        }
        root.append(group);
      }
    });
  }

  // The lists load once, when the group is made: the page does not read them again before the next load of the dashboard.
  links.forEach((link, n) => {
    if (link.kind !== "dashboards") return;
    loadItems(link).then(
      (list) => {
        if (destroyed) return;
        items.set(n, list);
        paint();
      },
      (error) => {
        // A failed request gives no item for this link (9.8a, step 5).
        console.warn(`The dashboard link ${n} could not read the list of dashboards: ${error && error.message ? error.message : error}`);
        if (!destroyed) items.set(n, []);
        if (!destroyed) paint();
      },
    );
  });
  paint();
  return {
    el: root,
    update: paint,
    destroy() {
      destroyed = true;
      root.remove();
    },
  };
}
