// The checks of the content of a text panel in the editor (10.3, 4.33a, 9.19b). Pure module: no DOM access.
// The lines are the array `options.content`. The editor splits the text of the field at each line end (`linesOf`).

import { MAX_TEXT_BYTES, MAX_TEXT_LINES, MAX_TEXT_LINE_BYTES, MD_MESSAGES, parseMarkdown } from "../markdown.js";

const CONTROL = /[\u0000-\u0008\u000a-\u001f\u007f-\u009f]/;
const utf8 = new TextEncoder();
const count = (n) => n.toLocaleString("en-US");

/** The lines of the text of the field: each CR LF and each lone CR is a line end. An empty text has no line. */
export const linesOf = (text) => (text === "" ? [] : text.replace(/\r\n?/g, "\n").split("\n"));

/** The text of the lines, as the page shows it in the field. */
export const textOf = (lines) => lines.join("\n");

/** The line "<n> of 65,536 bytes, <m> of 2,000 lines" (the bytes of the joined text, 4.33a rule 2). */
export function countText(lines) {
  const bytes = lines.length ? utf8.encode(textOf(lines)).length : 0;
  return `${count(bytes)} of ${count(MAX_TEXT_BYTES)} bytes, ${count(lines.length)} of ${count(MAX_TEXT_LINES)} lines`;
}

/** The first error of the lines, or "". The order is the order of 10.3: a control character, a long line, the counts, the limits of the page. */
export function firstTextError(lines) {
  for (let i = 0; i < lines.length; i++) if (CONTROL.test(lines[i])) return `Line ${i + 1} has a control character.`;
  for (let i = 0; i < lines.length; i++) if (lines[i].length * 3 > MAX_TEXT_LINE_BYTES && utf8.encode(lines[i]).length > MAX_TEXT_LINE_BYTES) return `Line ${i + 1} has more than ${count(MAX_TEXT_LINE_BYTES)} bytes.`;
  const parsed = parseMarkdown(lines);
  return parsed.ok ? "" : MD_MESSAGES[parsed.limit];
}
