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
