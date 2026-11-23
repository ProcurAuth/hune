// A JSON text area with a line number gutter, in the same frame as the SQL text area (10.3, 10.4).
// `input` is the real <textarea>. After a change of `input.value` from code, call `paint()`; typing repaints by itself.

import { el } from "../ui.js";

export function createJsonArea({ label, testid, tall = false }) {
  // The gutter is a frame with a fixed height. The numbers are in `nums`, which moves with the scroll position of the text area.
  const nums = el("pre", { class: "jarea__nums" });
  const gutter = el("div", { class: "jarea__gutter", "aria-hidden": "true" }, nums);
  const input = el("textarea", { class: "jarea__input", spellcheck: "false", autocorrect: "off", wrap: "off", "aria-label": label, autocapitalize: "off", autocomplete: "off", dataset: { testid } });
  const root = el("div", { class: `jframe${tall ? " jframe--tall" : ""}` }, gutter, input);
  let count = 0;

  function paint() {
    const lines = input.value === "" ? 1 : input.value.split("\n").length;
    if (lines !== count) {
      count = lines;
      nums.textContent = Array.from({ length: lines }, (_, i) => String(i + 1)).join("\n");
    }
    sync();
  }

  function sync() {
    nums.style.transform = `translateY(${-input.scrollTop}px)`;
  }

  input.addEventListener("scroll", sync);
  input.addEventListener("input", paint);
  return { el: root, input, paint };
}
