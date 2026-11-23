// Placeholders of a SQL text (9.10, 7.6). Pure module with no DOM access.
// The scan keeps the union of the names of three readers: the copied lexer (7.5), sql_lex_pg.js (7.20) and a reader of
// the strings of a JSON text for MongoDB (7.34). It never finds one name less than a run (7.19). A placeholder is not
// read across a string.
// Forms: $name, ${name} and ${name:format}, and the query parameter {name:Type} of ClickHouse (7.6a). A name is the
// longest run of [A-Za-z0-9_].
// A name that starts with "__" is a macro (7.10), not a variable. The form [[name]] is plain text.

import { sqlLex } from "./vendor/chdash/app_sql_lex.js";
import { isCodeKind, isCommentKind, lexPg } from "./sql_lex_pg.js";

// The formats of 7.9. A placeholder with another format is not read.
// The formats after "raw" are valid only inside a string (D7). The server refuses them in code.
// The scan reads them in all places, so a variable that a refused text names stays a dependency.
const FORMATS = new Set([
  "csv",
  "singlequote",
  "sqlstring",
  "raw",
  "json",
  "pipe",
  "regex",
  "glob",
  "lucene",
  "percentencode",
  "queryparam",
  "distributed",
  "doublequote",
  "html",
  "text",
]);

// The formats that only a JSON text of MongoDB has (7.34). A SQL text does not read them.
const FORMATS_JSON = new Set([...FORMATS, "number", "date", "oid", "bool", "json_number", "json_oid"]);

const PLACEHOLDER = /\$(?:\{([A-Za-z0-9_]+)(?::([A-Za-z0-9_]+))?\}|([A-Za-z0-9_]+))/g;

// The text of a list of SQL lines (4.16): the lines joined by one line feed.
export function sqlText(lines) {
  return Array.isArray(lines) ? lines.join("\n") : String(lines ?? "");
}

// The segments that can hold a placeholder, with the index of their first character: the code runs between the spans,
// and the strings and quoted names. The lexer is the copied lexer of the dialect `clickhouse`.
function segments(text) {
  const out = [];
  let at = 0;
  for (const span of sqlLex.spans(text)) {
    if (span.start > at) out.push({ text: text.slice(at, span.start), at });
    if (span.kind !== sqlLex.LINE && span.kind !== sqlLex.BLOCK) out.push({ text: text.slice(span.start, span.end), at: span.start });
    at = span.end;
  }
  if (at < text.length) out.push({ text: text.slice(at), at });
  return out;
}

// The same segments with the lexer of the dialect `postgres` (7.20). A run of code tokens is one segment, because a
// placeholder such as ${name:csv} is more than one token. A string, a quoted name and a dollar quote is one segment.
// A comment is left out.
function segmentsPg(text) {
  const out = [];
  let run = -1;
  let runEnd = 0;
  const flush = () => {
    if (run >= 0) out.push({ text: text.slice(run, runEnd), at: run });
    run = -1;
  };
  for (const token of lexPg(text)) {
    if (isCodeKind(token.kind)) {
      if (run < 0) run = token.start;
      runEnd = token.end;
      continue;
    }
    flush();
    if (!isCommentKind(token.kind)) out.push({ text: text.slice(token.start, token.end), at: token.start });
  }
  flush();
  return out;
}

// The third reader (7.19, 7.34): each string of a JSON text, keys too, with its escapes decoded. A string that is not
// valid JSON stays as written. A string with no end reads to the end of the text.
function segmentsJson(text) {
  return [...text.matchAll(/"(?:[^"\\]+|\\[^])*"?/g)].map((m) => {
    let s = m[0];
    try {
      s = JSON.parse(s);
    } catch {
      // keep the text as written
    }
    return { text: s, at: m.index };
  });
}

// The placeholders of the segments: [{ name, at }], where `at` is the index of the "$" in the text.
function namesOf(list, formats = FORMATS) {
  const found = [];
  for (const segment of list) {
    PLACEHOLDER.lastIndex = 0;
    for (let m = PLACEHOLDER.exec(segment.text); m; m = PLACEHOLDER.exec(segment.text)) {
      const at = segment.at + m.index;
      if (m[1] !== undefined) {
        if (m[2] !== undefined && !formats.has(m[2])) continue;
        found.push({ name: m[1], at });
      } else {
        found.push({ name: m[3], at });
      }
    }
  }
  return found;
}

// The query parameters of ClickHouse (7.6a): each `{name:` in code whose brace has no "$" before it. The name scan of
// the server reads them too (7.19), so a preview sends the value of such a variable.
function paramsOf(text) {
  const found = [];
  const code = (from, to) => {
    for (const m of text.slice(from, to).matchAll(/\{([A-Za-z_]\w*):/g)) {
      const at = from + m.index;
      if (text[at - 1] !== "$" && !m[1].startsWith("__")) found.push({ name: m[1], at });
    }
  };
  let at = 0;
  for (const span of sqlLex.spans(text)) {
    if (span.start > at) code(at, span.start);
    at = span.end;
  }
  code(at, text.length);
  return found;
}

// All placeholder names in first-use order, with duplicates: the union of the scan of each dialect.
function allNames(text) {
  const s = String(text ?? "");
  const found = [...namesOf(segments(s)), ...namesOf(segmentsPg(s)), ...namesOf(segmentsJson(s), FORMATS_JSON), ...paramsOf(s)];
  found.sort((a, b) => a.at - b.at);
  return found.filter((f, i) => i === 0 || f.at !== found[i - 1].at || f.name !== found[i - 1].name).map((f) => f.name);
}

// The distinct variable names of a SQL text, in first-use order. Macro names (prefix "__") are left out.
export function findVariableNames(text) {
  return [...new Set(allNames(text).filter((name) => !name.startsWith("__")))];
}

// The distinct macro names of a SQL text, for example "__timeFilter", in first-use order.
export function findMacros(text) {
  return [...new Set(allNames(text).filter((name) => name.startsWith("__")))];
}

export function variableNamesOfLines(lines) {
  return findVariableNames(sqlText(lines));
}
