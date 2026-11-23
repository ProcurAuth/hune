// A list of tags as chips (4.5, 4.5a): each chip has a button "Remove". A text field adds a tag with the Enter key or the button "Add".
// A tag has 1 to 64 characters. A tag that exists gives the error "This tag exists." The list has a limit (4.35).

import { button, el, uniqueId } from "../../ui.js";
import { iconBtn, setInactive } from "./controls.js";
import { limitText } from "./model.js";
import { checkTag } from "./rules.js";

/** `get()` and `set(array)` read and write the array of texts. `testid` names the hooks: `<testid>`, `<testid>-input`, `<testid>-add`, `<testid>-<n>`. */
export function tagsEditor({ label, get, set, limit, testid }) {
  const root = el("div", { class: "field tags", dataset: { testid } });
  const labelId = uniqueId("tags");
  const input = el("input", { class: "input", type: "text", spellcheck: "false", autocomplete: "off", id: labelId, dataset: { testid: `${testid}-input` } });
  const err = el("p", { class: "field-error", role: "alert", hidden: true });
  const show = (text) => {
    err.textContent = text || "";
    err.hidden = !text;
    if (text) input.setAttribute("aria-invalid", "true");
    else input.removeAttribute("aria-invalid");
  };
  const list = el("ul", { class: "tags__list", "aria-label": label });
  const addButton = button("Add", { dataset: { testid: `${testid}-add` }, on: { click: () => add() } }, "plus", { size: "sm" });

  function draw() {
    const tags = get();
    list.replaceChildren(...tags.map((t, i) => el("li", { class: "tags__chip", dataset: { testid: `${testid}-${i}` } },
      el("span", { class: "tags__text", text: t }),
      iconBtn(`Remove tag ${t}`, "close", () => {
        set(get().filter((_, k) => k !== i));
        show("");
        draw();
        input.focus();
      }, { testid: `${testid}-${i}-remove` }))));
    setInactive(addButton, tags.length >= limit, limitText(limit, "tags"));
  }

  function add() {
    const text = input.value.trim();
    if (text === "") {
      show("");
      return;
    }
    const tags = get();
    const error = tags.length >= limit ? limitText(limit, "tags") : checkTag(text, tags);
    show(error);
    if (error) return;
    set([...tags, text]);
    input.value = "";
    draw();
  }

  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      add();
    }
  });
  input.addEventListener("input", () => show(""));
  draw();
  root.append(el("label", { class: "field__label", for: labelId, text: label }), list, el("div", { class: "tags__add" }, input, addButton), err);
  return root;
}
