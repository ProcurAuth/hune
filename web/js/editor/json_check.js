// A JSON scanner for the JSON tabs (10.3, 10.4). It finds the syntax faults and the duplicate members with their line and
// column, and the place of each member by JSON pointer. Pure module: no DOM access.

const WS = new Set([" ", "\t", "\n", "\r"]);

export function lineColumn(text, index) {
  let line = 1;
  let col = 1;
  for (let i = 0; i < index && i < text.length; i++) {
    if (text[i] === "\n") {
      line += 1;
      col = 1;
    } else col += 1;
  }
  return { line, column: col };
}

const escapePointer = (s) => s.replace(/~/g, "~0").replace(/\//g, "~1");

class Syntax extends Error {
  constructor(message, index) {
    super(message);
    this.index = index;
  }
}

/**
 * Scans a text. Result: { ok, value, errors, positions }. `errors` has items of the form of 4.37 (pointer, code, message,
 * line, column). A syntax fault stops the scan. A duplicate member does not stop it.
 * `positions` maps a pointer to { keyStart, start, end } (indices in the text).
 */
export function scanJson(text) {
  const errors = [];
  const positions = new Map();
  let i = 0;

  const skip = () => {
    while (i < text.length && WS.has(text[i])) i++;
  };

  function parseString() {
    const start = i;
    if (text[i] !== '"') throw new Syntax("a text starts with a quote", i);
    i++;
    while (i < text.length) {
      const c = text[i];
      if (c === '"') {
        i++;
        return JSON.parse(text.slice(start, i));
      }
      if (c === "\\") {
        const n = text[i + 1];
        if (n === "u") {
          if (!/^[0-9a-fA-F]{4}$/.test(text.slice(i + 2, i + 6))) throw new Syntax("a \\u escape needs four hexadecimal digits", i);
          i += 6;
        } else if ('"\\/bfnrt'.includes(n) && n !== undefined) i += 2;
        else throw new Syntax("a bad escape in a text", i);
      } else if (c < " ") throw new Syntax("a control character in a text", i);
      else i++;
    }
    throw new Syntax("a text is not closed", start);
  }

  function parseValue(pointer, depth) {
    if (depth > 200) throw new Syntax("the JSON is too deep", i);
    skip();
    const start = i;
    const c = text[i];
    let value;
    if (c === "{") {
      i++;
      value = {};
      const seen = new Set();
      skip();
      if (text[i] === "}") i++;
      else {
        for (;;) {
          skip();
          const keyStart = i;
          const key = parseString();
          skip();
          if (text[i] !== ":") throw new Syntax("a colon is missing after a member name", i);
          i++;
          const child = `${pointer}/${escapePointer(key)}`;
          if (seen.has(key)) {
            const { line, column } = lineColumn(text, keyStart);
            errors.push({ pointer: child, code: "duplicate_member", message: `member ${key} occurs two times`, line, column });
          }
          seen.add(key);
          const v = parseValue(child, depth + 1);
          const entry = positions.get(child);
          if (entry) entry.keyStart = keyStart;
          Object.defineProperty(value, key, { value: v, enumerable: true, writable: true, configurable: true });
          skip();
          if (text[i] === ",") {
            i++;
            continue;
          }
          if (text[i] === "}") {
            i++;
            break;
          }
          throw new Syntax("a comma or a closing brace is expected", i);
        }
      }
    } else if (c === "[") {
      i++;
      value = [];
      skip();
      if (text[i] === "]") i++;
      else {
        for (let n = 0; ; n++) {
          value.push(parseValue(`${pointer}/${n}`, depth + 1));
          skip();
          if (text[i] === ",") {
            i++;
            continue;
          }
          if (text[i] === "]") {
            i++;
            break;
          }
          throw new Syntax("a comma or a closing bracket is expected", i);
        }
      }
    } else if (c === '"') value = parseString();
    else {
      const m = /^(?:-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?|true|false|null)/.exec(text.slice(i, i + 400));
      if (!m) throw new Syntax(c === undefined ? "the text ends too early" : "a value is expected", i);
      i += m[0].length;
      value = JSON.parse(m[0]);
    }
    const entry = positions.get(pointer) || {};
    entry.start = start;
    entry.end = i;
    if (entry.keyStart === undefined) entry.keyStart = start;
    positions.set(pointer, entry);
    return value;
  }

  try {
    const value = parseValue("", 0);
    skip();
    if (i < text.length) throw new Syntax("there is text after the end of the value", i);
    return { ok: errors.length === 0, value, errors, positions };
  } catch (error) {
    if (!(error instanceof Syntax)) throw error;
    const { line, column } = lineColumn(text, error.index);
    errors.push({ pointer: "", code: "syntax", message: `JSON syntax: ${error.message}`, line, column });
    return { ok: false, value: undefined, errors, positions };
  }
}

// The index of a pointer in the text. When the member is absent, the nearest ancestor that exists. Else 0.
export function locatePointer(positions, pointer) {
  let p = pointer;
  for (;;) {
    const entry = positions.get(p);
    if (entry) return entry.keyStart;
    const cut = p.lastIndexOf("/");
    if (cut < 0) return 0;
    p = p.slice(0, cut);
  }
}

// The text of a document with an indent of two spaces (10.4, when no canonical text exists).
export function prettyJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

// --- Colour and change marks of the JSON area (10.4) ------------------------------------------------------------------------
// Both parts are pure and work on text that is not valid: a person types in the area, so the text is often a draft. A JSON string never holds
// a line feed, so each line has its own colours and the window of drawn rows needs no state from the rows above it.

/** The most tokens that one line gets. A line with more tokens (a document of one line) keeps its text, and its end has no colour. */
export const MAX_TOKENS_PER_LINE = 2000;

const NUMBER = /-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?/y;
const WORD = /[A-Za-z]+/y;

/**
 * The coloured parts of one line, in order: [start, end, kind]. The kind is `key` (a text before a colon), `str`, `num`, `bool`, `null` or `punct`.
 * A text with no closing quote runs to the end of the line. Other characters, and white space, have no token (they stay plain text).
 * This is a lexer for colours. It does not decide that a document is valid: `scanJson` does.
 */
export function lineTokens(line) {
  const tokens = [];
  let i = 0;
  while (i < line.length && tokens.length < MAX_TOKENS_PER_LINE) {
    const c = line[i];
    if (c === '"') {
      let j = i + 1;
      while (j < line.length && line[j] !== '"') j += line[j] === "\\" ? 2 : 1;
      const end = Math.min(j + 1, line.length);
      let k = end;
      while (k < line.length && (line[k] === " " || line[k] === "\t")) k++;
      tokens.push([i, end, line[k] === ":" && j < line.length ? "key" : "str"]);
      i = end;
    } else if (c === "-" || (c >= "0" && c <= "9")) {
      NUMBER.lastIndex = i;
      const m = NUMBER.exec(line);
      if (m) {
        tokens.push([i, i + m[0].length, "num"]);
        i += m[0].length;
      } else i++;
    } else if (c === "{" || c === "}" || c === "[" || c === "]" || c === "," || c === ":") {
      tokens.push([i, i + 1, "punct"]);
      i++;
    } else if ((c >= "a" && c <= "z") || (c >= "A" && c <= "Z")) {
      WORD.lastIndex = i;
      const word = WORD.exec(line)[0];
      if (word === "true" || word === "false") tokens.push([i, i + word.length, "bool"]);
      else if (word === "null") tokens.push([i, i + word.length, "null"]);
      i += word.length;
    } else i++;
  }
  return tokens;
}

/** The width of a tab in the JSON area, in columns. The area has `tab-size: 2`, and the indent guides are two columns apart. */
export const TAB_SIZE = 2;

/** The number of columns that the white space at the start of a line takes. A tab goes to the next multiple of TAB_SIZE. */
export function indentColumns(line) {
  let col = 0;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === " ") col++;
    else if (c === "\t") col += TAB_SIZE - (col % TAB_SIZE);
    else break;
  }
  return col;
}

/** The number of characters of white space at the start of a line. */
export function indentLength(line) {
  let i = 0;
  while (i < line.length && (line[i] === " " || line[i] === "\t")) i++;
  return i;
}

/**
 * The brackets `{ } [ ]` of a document, found in one pass over its lines. A bracket in a text between quotes does not count: the text runs to its
 * closing quote or to the end of the line, as in `lineTokens`. The result has, for the brackets in the order of the text:
 *   pos     the offset of the bracket in the text (the lines are joined by one line feed),
 *   mate    the index of the other bracket of its pair, or -1 for a bracket with no pair,
 *   depth   the number of brackets that are open around it (a pair has one depth),
 *   parent  the index of the open bracket that holds its pair, or -1,
 * and for the lines: lineStart (the offset of each line, and the length of the text at the end) and lineFirst (the index of the first bracket of
 * each line, and the count at the end). A close bracket that does not match the open bracket on top has no pair and does not close it.
 */
export function scanBrackets(lines) {
  const n = lines.length;
  const lineStart = new Int32Array(n + 1);
  const lineFirst = new Int32Array(n + 1);
  const pos = [];
  const kind = [];
  const mate = [];
  const depth = [];
  const parent = [];
  const stack = [];
  let offset = 0;
  for (let r = 0; r < n; r++) {
    lineStart[r] = offset;
    lineFirst[r] = pos.length;
    const line = lines[r];
    for (let i = 0; i < line.length; i++) {
      const c = line.charCodeAt(i);
      if (c === 34) {
        i++;
        while (i < line.length && line.charCodeAt(i) !== 34) i += line.charCodeAt(i) === 92 ? 2 : 1;
      } else if (c === 123 || c === 91) {
        stack.push(pos.length);
        pos.push(offset + i);
        kind.push(c);
        mate.push(-1);
        depth.push(stack.length - 1);
        parent.push(stack.length > 1 ? stack[stack.length - 2] : -1);
      } else if (c === 125 || c === 93) {
        const top = stack.length ? stack[stack.length - 1] : -1;
        const index = pos.length;
        pos.push(offset + i);
        kind.push(c);
        if (top >= 0 && kind[top] === c - 2) {
          stack.pop();
          mate.push(top);
          mate[top] = index;
          depth.push(depth[top]);
          parent.push(parent[top]);
        } else {
          mate.push(-1);
          depth.push(0);
          parent.push(-1);
        }
      }
    }
    offset += line.length + 1;
  }
  lineStart[n] = offset;
  lineFirst[n] = pos.length;
  return { lineStart, lineFirst, pos: Int32Array.from(pos), mate: Int32Array.from(mate), depth: Uint16Array.from(depth), parent: Int32Array.from(parent), count: pos.length };
}

const BRACKET_CLASS = ["jt-b0", "jt-b1", "jt-b2"];

/** The css class of the bracket with the index `index` of a scan: its colour follows its depth (three colours that repeat), and a bracket with no pair has the colour of an error. */
export function bracketClass(scan, index) {
  return scan.mate[index] < 0 ? "jt-bb" : BRACKET_CLASS[scan.depth[index] % BRACKET_CLASS.length];
}

/** The index of the line that holds an offset: the last line that starts at or before it. */
export function rowOf(scan, offset) {
  const starts = scan.lineStart;
  let lo = 0;
  let hi = starts.length - 2;
  if (hi < 0) return 0;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid] <= offset) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

// The index of the bracket at an offset, or -1.
function bracketAt(scan, offset) {
  let lo = 0;
  let hi = scan.count - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const p = scan.pos[mid];
    if (p === offset) return mid;
    if (p < offset) lo = mid + 1;
    else hi = mid - 1;
  }
  return -1;
}

/** The bracket that the caret touches: the one before the caret first, else the one after it. It gives the index, or -1. */
export function bracketNear(scan, caret) {
  const before = caret > 0 ? bracketAt(scan, caret - 1) : -1;
  return before >= 0 ? before : bracketAt(scan, caret);
}

/** The pair of brackets that holds a caret: [open, close] as indexes, or null when no pair holds it (the document level, or a bracket with no pair). */
export function enclosingPair(scan, caret) {
  // The last bracket before the caret.
  let lo = 0;
  let hi = scan.count - 1;
  let last = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (scan.pos[mid] < caret) {
      last = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  if (last < 0 || scan.mate[last] < 0) return null;
  // `last` is an open bracket whose pair is after the caret (it holds the caret), or a close bracket (its pair is closed: the pair around it holds the caret).
  const around = scan.mate[last] > last ? last : scan.parent[scan.mate[last]];
  return around >= 0 && scan.mate[around] >= 0 ? [around, scan.mate[around]] : null;
}

/**
 * The part of a line to underline for a syntax error at `column` (the first column is 1): the token that holds the column, else the word of
 * non-space characters that starts at the first non-space character from it, else the last non-space character of the line. It gives [start, end) or null.
 */
export function errorRange(line, tokens, column) {
  if (!line.length || !Number.isInteger(column) || column < 1) return null;
  let at = Math.min(column - 1, line.length);
  for (const [start, end] of tokens) if (at >= start && at < end) return [start, end];
  while (at < line.length && (line[at] === " " || line[at] === "\t")) at++;
  if (at >= line.length) {
    let k = line.length;
    while (k > 0 && (line[k - 1] === " " || line[k - 1] === "\t")) k--;
    return k > 0 ? [k - 1, k] : null;
  }
  let end = at + 1;
  while (end < line.length && end - at < 40 && line[end] !== " " && line[end] !== "\t" && !"{}[],:\"".includes(line[end])) end++;
  return [at, end];
}

/** The most cells (rows of the saved text times rows of the new text, after the common start and end are cut) that the line diff computes. */
export const MAX_DIFF_CELLS = 1_000_000;

// The steps of the shortest edit between two arrays of lines: [type, indexInA, indexInB] with type "=", "-" or "+". Longest common subsequence.
function editSteps(a, b) {
  const n = a.length;
  const m = b.length;
  const w = m + 1;
  const table = new Uint32Array((n + 1) * w);
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      table[i * w + j] = a[i] === b[j] ? table[(i + 1) * w + j + 1] + 1 : Math.max(table[(i + 1) * w + j], table[i * w + j + 1]);
    }
  }
  const steps = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      steps.push(["=", i++, j++]);
    } else if (table[(i + 1) * w + j] >= table[i * w + j + 1]) steps.push(["-", i++, j]);
    else steps.push(["+", i, j++]);
  }
  while (i < n) steps.push(["-", i++, j]);
  while (j < m) steps.push(["+", i, j++]);
  return steps;
}

/**
 * The lines of `text` that differ from `savedText`, for the gutter. The result is { added, modified, deleted }, three arrays of line indexes
 * in `text` (the first line is 0), each in order:
 *   - `modified`: a line that replaces a line of the saved text;
 *   - `added`: a line that has no line of the saved text in its place;
 *   - `deleted`: the index of the line before which one or more saved lines are gone (the number of lines when they were the last ones).
 * The common start and the common end are cut first, so a small change in a long text costs little. When the rest is larger than
 * MAX_DIFF_CELLS cells (a change of more than 1,000 lines at the start and at the end both), the lines are compared in pairs by their place: a line
 * that moved shows as modified, and no changed line goes without a mark.
 */
export function diffLines(savedText, text) {
  const result = { added: [], modified: [], deleted: [] };
  if (savedText === text) return result;
  const a = savedText.split("\n");
  const b = text.split("\n");
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }
  const midA = a.slice(start, endA);
  const midB = b.slice(start, endB);
  let steps;
  if (midA.length * midB.length <= MAX_DIFF_CELLS) steps = editSteps(midA, midB);
  else {
    steps = [];
    const common = Math.min(midA.length, midB.length);
    for (let k = 0; k < common; k++) steps.push(...(midA[k] === midB[k] ? [["=", k, k]] : [["-", k, k], ["+", k, k]]));
    for (let k = common; k < midA.length; k++) steps.push(["-", k, midB.length]);
    for (let k = common; k < midB.length; k++) steps.push(["+", midA.length, k]);
  }
  // A run of steps that are not "=" is one change: its removed lines and its added lines pair up as modified lines, then the rest.
  let s = 0;
  while (s < steps.length) {
    if (steps[s][0] === "=") {
      s++;
      continue;
    }
    let removed = 0;
    const added = [];
    for (; s < steps.length && steps[s][0] !== "="; s++) {
      if (steps[s][0] === "-") removed++;
      else added.push(start + steps[s][2]);
    }
    const paired = Math.min(removed, added.length);
    for (let k = 0; k < added.length; k++) (k < paired ? result.modified : result.added).push(added[k]);
    if (removed > added.length) result.deleted.push(added.length ? added[added.length - 1] + 1 : start + (s < steps.length ? steps[s][2] : midB.length));
  }
  return result;
}
