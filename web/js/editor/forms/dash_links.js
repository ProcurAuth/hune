// The form of the dashboard links (10.3b, 4.5a): the tab "Links" of the settings view.
// A `link` row has a title, a URL and three switches. A `dashboards` row has a title, a list of tags and four switches.

import { el } from "../../ui.js";
import { listEditor, switchField, textField } from "./controls.js";
import { LIMITS, newDashboardLink } from "./model.js";
import { tagsEditor } from "./tags.js";
import { checkChars, checkUrlTemplate } from "./rules.js";

const KINDS = [["link", "Link"], ["dashboards", "Dashboards"]];
export const DROPDOWN_TITLE_ERROR = "A dropdown needs a title of 1 character or more.";

/** `get()` and `set(array)` read and write the top-level `links`. The hooks are `settings-links`, `settings-link-<n>` and `settings-link-add`. */
export function dashLinksForm(scope, { get, set }) {
  return listEditor(scope, {
    testid: "settings-links",
    itemTestid: (i) => `settings-link-${i}`,
    get,
    set,
    limit: LIMITS.dashboardLinks,
    limitWhat: "dashboard links",
    emptyText: "No dashboard link.",
    title: (l, i) => (l.kind === "link" ? `Link: ${l.title}` : `Dashboards: ${l.title || `tags ${l.tags.join(", ") || "all"}`}`) || `Link ${i + 1}`,
    add: { label: "Add link", testid: "settings-link-add", kinds: KINDS, kindLabel: "Kind of the link", make: (kind) => newDashboardLink(kind) },
    renderItem(l, i, ctx) {
      const t = (name) => `${ctx.testid}-${name}`;
      const flags = (names) => names.map(([key, label]) => switchField({ label, checked: l[key], testid: t(key), apply: (v) => onFlag(key, v) }).node);
      let titleField = null;
      const onFlag = (key, v) => {
        ctx.update({ [key]: v });
        if (key === "as_dropdown" && titleField) titleField.error(v && get()[i].title === "" ? DROPDOWN_TITLE_ERROR : "");
      };
      if (l.kind === "link") {
        titleField = textField(scope, { label: "Title", value: l.title, check: (v) => checkChars(v, 1, 256, "The title"), apply: (v) => ctx.update({ title: v }), testid: t("title") });
        return [
          titleField.node,
          textField(scope, { label: "URL", value: l.url, mono: true, check: checkUrlTemplate, apply: (v) => ctx.update({ url: v }), testid: t("url") }).node,
          ...flags([["target_blank", "Open in a new tab"], ["include_vars", "Include the variables"], ["keep_time", "Keep the time range"]]),
        ];
      }
      titleField = textField(scope, {
        label: "Title",
        value: l.title,
        check: (v) => (get()[i].as_dropdown && v === "" ? DROPDOWN_TITLE_ERROR : checkChars(v, 0, 256, "The title")),
        apply: (v) => ctx.update({ title: v }),
        testid: t("title"),
      });
      const tags = tagsEditor({ label: "Tags", get: () => get()[i].tags, set: (next) => ctx.update({ tags: next }), limit: LIMITS.linkTags, testid: t("tags") });
      return [
        titleField.node,
        tags,
        el("p", { class: "field__hint", text: "A link shows each dashboard that has all the tags. No tag shows each dashboard." }),
        ...flags([["as_dropdown", "Show as a dropdown"], ["target_blank", "Open in a new tab"], ["include_vars", "Include the variables"], ["keep_time", "Keep the time range"]]),
      ];
    },
  });
}
