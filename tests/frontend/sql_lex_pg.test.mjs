// The lexer of the dialect `postgres` of the page (7.20) gives the same tokens as the lexer of the server.
// The vectors are tests/fixtures/sql-lex-vectors-pg.json, which the Rust test of the server reads too.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { KIND, lexPg } from "../../web/js/sql_lex_pg.js";

// The letters of 7.5 and 7.20. A lower case letter marks a token that does not close.
const LETTERS = {
  [KIND.BLANK]: "_",
  [KIND.LINE]: "L",
  [KIND.BLOCK]: "B",
  [KIND.WORD]: "W",
  [KIND.NAME]: "Q",
  [KIND.STRING]: "S",
  [KIND.PREFIXED]: "S",
  [KIND.ESTRING]: "E",
  [KIND.HEREDOC]: "H",
  [KIND.NUMBER]: "N",
  [KIND.PUNCT]: "P",
  [KIND.OPERATOR]: "O",
  [KIND.PARAM]: "R",
};
const FAULTS = { [KIND.BLOCK]: "UnclosedBlockComment", [KIND.NAME]: "UnclosedQuotedName", [KIND.HEREDOC]: "UnclosedHeredoc" };

function letters(text) {
  return lexPg(text)
    .map((t) => (t.closed ? LETTERS[t.kind] : LETTERS[t.kind].toLowerCase()))
    .join("");
}

// The first lexer fault, as the server names it (7.26), or null.
function firstFault(text) {
  const t = lexPg(text).find((x) => !x.closed);
  return t ? (FAULTS[t.kind] ?? "UnclosedString") : null;
}

const vectors = JSON.parse(readFileSync(new URL("../fixtures/sql-lex-vectors-pg.json", import.meta.url), "utf8"));

test("the vector file has the members that the test reads", () => {
  assert.ok(vectors.length >= 150, `${vectors.length} vectors`);
  for (const v of vectors) {
    assert.equal(typeof v.sql, "string");
    assert.equal(typeof v.tokens, "string");
    assert.ok(v.error === null || typeof v.error === "string");
  }
});

for (const [i, v] of vectors.entries()) {
  test(`vector ${i + 1}: ${JSON.stringify(v.sql)}`, () => {
    const tokens = lexPg(v.sql);
    // The tokens cover the text with no gap.
    let at = 0;
    for (const t of tokens) {
      assert.equal(t.start, at, "gap");
      assert.ok(t.end > t.start, "empty token");
      at = t.end;
    }
    assert.equal(at, v.sql.length);
    assert.equal(tokens.map((t) => v.sql.slice(t.start, t.end)).join(""), v.sql);
    assert.equal(letters(v.sql), v.tokens, v.note);
    assert.equal(firstFault(v.sql), v.error, `error: ${v.note}`);
  });
}

test("the table of 7.20", () => {
  const table = [
    ["SELECT 1", "W_N"],
    ["'a''b'", "S"],
    ["1 # x", "N_O_W"],
    ["$1", "R"],
    ["$tag$a$$b$tag$", "H"],
    ["$$a'b$$", "H"],
    ["a$b", "W"],
    ['U&"a"', "Q"],
    ["x@-1", "WON"],
    ["a<-b", "WOOW"],
    ["'a' 'b'", "S_S"],
    ["1 2", "NW"],
    ["'a\\'b'", "Ss"],
    ["E'a\\'b'", "E"],
    ["1 --x", "N_L"],
    ["/* a /* b */ c */", "B"],
  ];
  for (const [sql, want] of table) assert.equal(letters(sql), want, sql);
});

test("a prefix is read only at the start of a token", () => {
  assert.equal(letters("xE'a'"), "WS");
  assert.equal(letters("E'a'"), "E");
});

test("a continuation of an E string keeps the escape and the kind", () => {
  const sql = "E'a'\n'b\\'c'";
  const kinds = lexPg(sql).map((t) => t.kind);
  assert.deepEqual(kinds, [KIND.ESTRING, KIND.BLANK, KIND.ESTRING]);
  assert.equal(letters(sql), "E_E");
});

test("a text with multibyte characters keeps the positions of UTF-16", () => {
  const sql = "SELECT 'é€😀' , \"ñ\" -- ü\nFROM t";
  assert.equal(lexPg(sql).map((t) => sql.slice(t.start, t.end)).join(""), sql);
  assert.equal(letters(sql), "W_S_P_Q_L_W_W");
});

test("random texts: the tokens always cover the text and the lexer never throws", () => {
  const pieces = ["'", '"', "$", "$$", "$a$", "E'", "U&", "U&'", "--", "/*", "*/", "\\", "\n", "\r", " ", "#", "-", "+", "1", "a", "é", "😀", "x'", "\t", ";", "@", "."];
  let seed = 12345;
  const next = () => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed;
  };
  for (let n = 0; n < 5000; n += 1) {
    let sql = "";
    const len = 1 + (next() % 14);
    for (let i = 0; i < len; i += 1) sql += pieces[next() % pieces.length];
    const tokens = lexPg(sql);
    let at = 0;
    for (const t of tokens) {
      assert.equal(t.start, at, JSON.stringify(sql));
      assert.ok(t.end > t.start, JSON.stringify(sql));
      at = t.end;
    }
    assert.equal(at, sql.length, JSON.stringify(sql));
  }
});
