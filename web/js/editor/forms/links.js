// The form of the data links and of the panel links (10.3a, 4.23): a list of rows with a title, a URL and the switch `target_blank`.
// It serves the groups "Data links" (`fields.defaults.links`) and "Panel links" (`links`), and the property `links` of an override.
// The dashboard links (4.5a) have their own form in dash_links.js.

import { listEditor, switchField, textField } from "./controls.js";
import { LIMITS, newLink } from "./model.js";
import { checkChars, checkUrlTemplate } from "./rules.js";

/**
 * `scopeName` is `defaults`, `panel` or an override name: the hooks are `links-<scopeName>` and `link-<scopeName>-<n>`.
 * `minItems`: 1 in an override, because the property `links` has 1 item or more (4.18).
 */
export function linksForm(scope, { scopeName, get, set, minItems = 0 }) {
  return listEditor(scope, {
    testid: `links-${scopeName}`,
    itemTestid: (i) => `link-${scopeName}-${i}`,
    get,
    set,
    limit: LIMITS.links,
    limitWhat: "links",
    minItems,
    removeHint: "A property needs one link. Remove the property instead.",
    emptyText: "No link.",
    title: (l, i) => l.title || `Link ${i + 1}`,
    add: { label: "Add link", testid: `link-${scopeName}-add`, make: () => newLink() },
    renderItem(l, i, ctx) {
      return [
        textField(scope, { label: "Title", value: l.title, check: (v) => checkChars(v, 1, 256, "The title"), apply: (v) => ctx.update({ title: v }), testid: `${ctx.testid}-title` }).node,
        textField(scope, { label: "URL", value: l.url, mono: true, check: checkUrlTemplate, apply: (v) => ctx.update({ url: v }), testid: `${ctx.testid}-url`, hint: "Tokens such as ${__value.raw} and ${var-name} are possible." }).node,
        switchField({ label: "Open in a new tab", checked: l.target_blank, apply: (v) => ctx.update({ target_blank: v }), testid: `${ctx.testid}-blank` }).node,
      ];
    },
  });
}
