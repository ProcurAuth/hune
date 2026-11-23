// The JSON tree of the query editor (F42): the vectors, the tolerance, the coverage of the tokens and of the brackets, the caps and the time.
import test from "node:test";
import assert from "node:assert/strict";
import { parseJson } from "../../web/js/editor/ast/json_tree.js";
import { MAX_AST_DEPTH, MAX_AST_INPUT_BYTES, MAX_AST_NODES, TOKEN, walk } from "../../web/js/editor/ast/nodes.js";

const KINDS = new Set(Object.values(TOKEN));

/** The nodes of a tree with a given type. */
function nodesOf(root, type) {
  const out = [];
  walk(root, (n) => {
    if (n.type === type) out.push(n);
  });
  return out;
}

/** The first node that satisfies a test. */
function find(root, test2) {
  let found = null;
  walk(root, (n) => {
    if (found) return false;
    if (test2(n)) found = n;
    return undefined;
  });
  return found;
}

/** The members of an object node, as a map of the name to the value node. */
function members(obj) {
  const out = {};
  for (const m of obj.children) if (m.type === "member") out[m.name] = m.children[m.children.length - 1];
  return out;
}

/** The value of a tree for a text of valid JSON. It must be equal to `JSON.parse`. */
function valueOf(text, n) {
  switch (n.type) {
    case "doc":
      return valueOf(text, n.children[0]);
    case "object": {
      const out = {};
      for (const m of n.children) Object.defineProperty(out, m.name, { value: valueOf(text, m.children[0]), enumerable: true, writable: true, configurable: true });
      return out;
    }
    case "array":
      return n.children.map((c) => valueOf(text, c));
    case "string":
    case "number":
    case "literal":
      return JSON.parse(text.slice(n.start, n.end));
    default:
      throw new Error(`type ${n.type} in a valid text`);
  }
}

/** Checks the invariants of a result for any text, valid or not. */
function checkInvariants(text, r, { brackets = true } = {}) {
  const { tokens, root } = r;
  // Token coverage: no gap, no overlap, known kinds.
  let at = 0;
  for (const t of tokens) {
    if (!KINDS.has(t.kind)) assert.fail(`kind ${t.kind}`);
    if (t.start !== at) assert.fail(`gap or overlap at ${at} in ${JSON.stringify(text)}`);
    assert.ok(t.end > t.start, "an empty token");
    assert.equal(typeof t.closed, "boolean");
    at = t.end;
  }
  assert.equal(at, text.length, "the tokens do not reach the end");
  // The nodes: the root spans the text, a child is in its parent, the siblings keep their order.
  assert.equal(root.type, "doc");
  assert.equal(root.start, 0);
  assert.equal(root.end, text.length);
  const opens = new Set();
  const closes = new Set();
  const strays = new Set();
  walk(root, (n) => {
    let last = n.start;
    for (const c of n.children) {
      if (c.start < last) assert.fail(`sibling order: ${c.type} at ${c.start} before ${last} in ${JSON.stringify(text)}`);
      assert.ok(c.end >= c.start);
      if (c.end > n.end) assert.fail(`child ${c.type} ${c.start}-${c.end} out of ${n.type} ${n.start}-${n.end} in ${JSON.stringify(text)}`);
      last = c.end;
    }
    if (n.type === "object" || n.type === "array") {
      assert.equal(n.open, n.start);
      assert.equal(text[n.open], n.type === "object" ? "{" : "[");
      opens.add(n.open);
      if (n.close !== undefined) {
        assert.equal(text[n.close], n.type === "object" ? "}" : "]");
        assert.equal(n.end, n.close + 1);
        closes.add(n.close);
        assert.notEqual(n.error, "unclosed");
      } else assert.equal(n.error, "unclosed");
    }
    if (n.type === "error" && n.error === "unmatched") strays.add(n.start);
    if (n.type === "member") assert.equal(typeof n.name, "string");
    if (n.foldable) assert.ok(text.slice(n.start, n.end).includes("\n"), "a fold on one line");
  });
  if (brackets && !r.capped) {
    for (const t of tokens) {
      if (t.kind !== "punct") continue;
      const c = text[t.start];
      if ((c === "{" || c === "[") && !opens.has(t.start)) assert.fail(`bracket at ${t.start} has no node`);
      if ((c === "}" || c === "]") && !closes.has(t.start) && !strays.has(t.start)) assert.fail(`bracket at ${t.start} has no node`);
    }
  }
}

/** Parses, checks the invariants, and gives the result. */
function parse(text, options) {
  const r = parseJson(text, options);
  checkInvariants(text, r);
  return r;
}

// ---- The samples ----

const PIPELINE = `{
  "collection": "orders",
  "aggregate": [
    {"$match": {"$expr": {"$and": [{"$gte": ["$ts", "$__timeFrom"]}, {"$lt": ["$ts", "$__timeTo"]}]}, "host": "\${host:json}"}},
    {"$lookup": {"from": "items", "let": {"id": "$_id"}, "pipeline": [
      {"$match": {"$expr": {"$eq": ["$$id", "$order"]}}},
      {"$project": {"_id": 0, "sku": 1}}
    ], "as": "items"}},
    {"$facet": {"a": [{"$count": "n"}], "b": [{"$group": {"_id": "$region", "total": {"$sum": "$amount"}}}, {"$sort": {"total": -1}}]}},
    {"$limit": 100}
  ]
}`;

const SAMPLES = [
  PIPELINE,
  "[]",
  "{}",
  '[{"$match": {"a": 1}}, {"$limit": 5}]',
  '{"a": [[1, 2], [3, [4, [5]]], []], "b": {"c": {"d": {}}}}',
  '{"s": "caf\\u00e9 \\ud83d\\ude00 \\n \\" \\\\ \\/", "t": "plain"}',
  '{"n": [0, -0, 1.5, -2.5e+10, 1E-3, 12345678901234567890123, 0.1e1]}',
  '{"v": ["$host", "${host}", "${host:csv}", "a-$x-b", "$__interval_ms", "${__timeFrom}"]}',
  '{"x": true, "y": false, "z": null, "w": [true, null]}',
  '[{"$group": {"_id": null, "n": {"$sum": 1}}}]',
  '{"find": {"filter": {"a": {"$in": "${h:json}"}}, "sort": {"a": -1}, "limit": 10}}',
  '{"a":{"b":[{"c":1},{"d":[2,3]}]}}',
  '  \n\t {"a" : 1 ,\n "b" : [ 1 , 2 ]\n}\n',
  '{"$match": {"name": "\\u0024host"}}',
  '[{"$set": {"a": "$$CURRENT.b"}}, {"$unset": ["a", "b"]}]',
  '{"a": "x", "a": "y"}',
  '{"k": "é中😀"}',
  '[1, 2,, 3]',
  '{"a": 1,}',
  '{"a" 1, "b": }',
];

// ---- Vectors ----

test("the stage objects of the pipeline get their stage", () => {
  const r = parse(PIPELINE);
  const stages = nodesOf(r.root, "object").filter((o) => o.stage).map((o) => o.stage);
  assert.deepEqual(stages, ["$match", "$lookup", "$match", "$project", "$facet", "$count", "$group", "$sort", "$limit"]);
  const stage = find(r.root, (n) => n.stage === "$lookup");
  assert.equal(stage.name, "$lookup");
  // The operator objects inside a stage are not stages.
  const expr = find(r.root, (n) => n.type === "member" && n.name === "$expr");
  assert.equal(expr.children[0].stage, undefined);
});

test("a stage needs one member whose key starts with a dollar", () => {
  const r = parse('[{"$match": {}, "$limit": 1}, {"a": 1}, {}, {"$limit": 1}]');
  const objects = r.root.children[0].children;
  assert.deepEqual(objects.map((o) => o.stage), [undefined, undefined, undefined, "$limit"]);
  // The top array of a document and the value of `aggregate` are pipelines. A plain array is not.
  const plain = parse('{"x": [{"$limit": 1}]}');
  assert.equal(nodesOf(plain.root, "object").filter((o) => o.stage).length, 0);
  const agg = parse('{"aggregate": [{"$limit": 1}]}');
  assert.equal(nodesOf(agg.root, "object").filter((o) => o.stage).length, 1);
  const nested = parse('{"a": {"aggregate": [{"$limit": 1}]}}');
  assert.equal(nodesOf(nested.root, "object").filter((o) => o.stage).length, 0);
});

test("the sub-pipelines of $lookup, $facet and $unionWith have stages", () => {
  const r = parse('[{"$lookup": {"pipeline": [{"$match": {}}]}}, {"$unionWith": {"coll": "c", "pipeline": [{"$skip": 1}]}}, {"$facet": {"f": [{"$count": "n"}], "g": [{"$limit": 2}]}}]');
  const stages = nodesOf(r.root, "object").filter((o) => o.stage).map((o) => o.stage);
  assert.deepEqual(stages, ["$lookup", "$match", "$unionWith", "$skip", "$facet", "$count", "$limit"]);
});

test("a valid text gives the same value as JSON.parse and no error", () => {
  for (const text of SAMPLES) {
    let expected;
    try {
      expected = JSON.parse(text);
    } catch {
      continue;
    }
    const r = parse(text);
    assert.equal(r.capped, null);
    walk(r.root, (n) => {
      if (n.type === "error") assert.fail(`error node ${n.error} in ${text}`);
      if (n.error && !(n.type === "member" && n.error === "duplicate_member")) assert.fail(`error ${n.error} in ${text}`);
    });
    assert.deepEqual(valueOf(text, r.root), expected, text);
  }
});

test("a pipeline with a sub-pipeline: members, names and positions", () => {
  const r = parse(PIPELINE);
  const top = r.root.children[0];
  assert.equal(top.type, "object");
  assert.equal(top.open, 0);
  assert.equal(top.close, PIPELINE.length - 1);
  assert.equal(top.foldable, true);
  assert.deepEqual(top.children.map((m) => m.name), ["collection", "aggregate"]);
  const agg = members(top).aggregate;
  assert.equal(agg.type, "array");
  assert.equal(agg.children.length, 4);
  const lookup = members(members(agg.children[1]).$lookup);
  assert.equal(lookup.pipeline.type, "array");
  assert.equal(lookup.pipeline.children.length, 2);
  assert.equal(lookup.pipeline.foldable, true);
  // The object that holds `{"$limit": 100}` is on one line: no fold.
  assert.equal(agg.children[3].foldable, undefined);
  assert.equal(PIPELINE.slice(agg.children[3].start, agg.children[3].end), '{"$limit": 100}');
});

test("$facet: an object of arrays of stages", () => {
  const r = parse(PIPELINE);
  const facet = members(find(r.root, (n) => n.stage === "$facet")).$facet;
  assert.deepEqual(facet.children.map((m) => m.name), ["a", "b"]);
  assert.equal(members(facet).b.children.length, 2);
});

test("$match with $expr, arrays of arrays, and empty containers", () => {
  const text = '{"$match": {"$expr": {"$and": [[1, [2]], [], {}]}}}';
  const r = parse(text);
  const and = find(r.root, (n) => n.type === "member" && n.name === "$and").children[0];
  assert.equal(and.type, "array");
  assert.deepEqual(and.children.map((c) => c.type), ["array", "array", "object"]);
  assert.equal(and.children[1].children.length, 0);
  assert.equal(and.children[2].children.length, 0);
});

test("the foldable flag: two lines or more", () => {
  const r = parse('{"a": [1,\n2], "b": {"c": 1}, "d": [\n]}');
  const top = r.root.children[0];
  assert.equal(top.foldable, true);
  const v = members(top);
  assert.equal(v.a.foldable, true);
  assert.equal(v.b.foldable, undefined);
  assert.equal(v.d.foldable, true);
  // An unclosed container that spans two lines folds too.
  const open = parse('[\n{"a": 1,\n"b": 2');
  assert.equal(open.root.children[0].foldable, true);
  assert.equal(open.root.children[0].children[0].foldable, true);
});

test("placeholders in strings and as whole values are tokens", () => {
  const text = '{"a": "$host", "b": "x${y:csv}z", "c": "$__timeFrom", "d": "${__to}", "e": "$", "f": "${x:bad}"}';
  const r = parse(text);
  const placeholders = r.tokens.filter((t) => t.kind === "variable" || t.kind === "macro").map((t) => [t.kind, text.slice(t.start, t.end)]);
  assert.deepEqual(placeholders, [
    ["variable", "$host"],
    ["variable", "${y:csv}"],
    ["macro", "$__timeFrom"],
    ["macro", "${__to}"],
  ]);
  // The tree keeps each string as one node, and it holds all its bytes.
  const strings = nodesOf(r.root, "string").map((s) => text.slice(s.start, s.end));
  assert.deepEqual(strings, ['"$host"', '"x${y:csv}z"', '"$__timeFrom"', '"${__to}"', '"$"', '"${x:bad}"']);
  // The pieces of one string: a quote, the placeholder, the rest.
  const k = r.tokens.findIndex((t) => text.slice(t.start, t.end) === "${y:csv}");
  assert.equal(text.slice(r.tokens[k - 1].start, r.tokens[k - 1].end), '"x');
  assert.equal(text.slice(r.tokens[k + 1].start, r.tokens[k + 1].end), 'z"');
});

test("a placeholder with a formats of MongoDB is read in a string and not outside", () => {
  const text = '{"a": "${x:number}", "b": ${y:number}}';
  const r = parse(text);
  const found = r.tokens.filter((t) => t.kind === "variable").map((t) => text.slice(t.start, t.end));
  assert.deepEqual(found, ["${x:number}"]);
});

test("a placeholder outside a string is a variable token and an error node", () => {
  const text = '{"a": $host, "b": $__from, "c": ${n}}';
  const r = parse(text);
  const kinds = r.tokens.filter((t) => t.kind === "variable" || t.kind === "macro").map((t) => t.kind);
  assert.deepEqual(kinds, ["variable", "macro", "variable"]);
  const errors = nodesOf(r.root, "error");
  assert.deepEqual(errors.map((e) => e.error), ["placeholder_outside_string", "placeholder_outside_string", "placeholder_outside_string"]);
  // Each error node is the value of its member: the structure holds.
  assert.deepEqual(members(r.root.children[0]).a.type, "error");
});

test("the option `variables` makes a field path plain text", () => {
  const text = '{"a": "$email", "b": "$host", "c": "$__from", "d": "${host:csv}"}';
  const r = parse(text, { variables: ["host"] });
  const found = r.tokens.filter((t) => t.kind === "variable" || t.kind === "macro").map((t) => text.slice(t.start, t.end));
  assert.deepEqual(found, ["$host", "$__from", "${host:csv}"]);
  const all = parse(text);
  assert.equal(all.tokens.filter((t) => t.kind === "variable").length, 3);
});

test("an escape that writes a placeholder is a placeholder, as in the scanner of the page", () => {
  const text = '{"a": "\\u0024host and \\u0024{x:csv}"}';
  const r = parse(text);
  const found = r.tokens.filter((t) => t.kind === "variable").map((t) => text.slice(t.start, t.end));
  assert.deepEqual(found, ["\\u0024host", "\\u0024{x:csv}"]);
});

test("unicode escapes and escapes in keys: the name is decoded", () => {
  const text = '{"caf\\u00e9": 1, "a\\"b": 2, "t\\tab": 3, "\\ud83d\\ude00": 4}';
  const r = parse(text);
  assert.deepEqual(r.root.children[0].children.map((m) => m.name), ["café", 'a"b', "t\tab", "\u{1f600}"]);
});

test("a string with a bad escape or a control character has an error and stays a string", () => {
  const bad = parse('["a\\qb", "a\\u12g4", "tab\there"]');
  const strings = nodesOf(bad.root, "string");
  assert.deepEqual(strings.map((s) => s.error), ["bad_escape", "bad_escape", "control_char"]);
  assert.equal(bad.tokens.filter((t) => t.kind === "string").length, 3);
});

test("a string ends at a raw line end: the next lines stay readable", () => {
  const text = '{"a": "open\n, "b": 2}';
  const r = parse(text);
  const s = nodesOf(r.root, "string")[0];
  assert.equal(s.error, "unclosed");
  assert.equal(text.slice(s.start, s.end), '"open');
  const t = r.tokens.find((k) => k.kind === "string" && text.slice(k.start, k.end) === '"open');
  assert.equal(t.closed, false);
  assert.equal(r.tokens.filter((k) => k.kind === "string" && !k.closed).length, 1);
});

test("big numbers are one number node, and a wrong number is an error node", () => {
  const text = '[123456789012345678901234567890, -1.5E+300, 1e999, 0, -0]';
  const r = parse(text);
  assert.deepEqual(r.root.children[0].children.map((c) => c.type), ["number", "number", "number", "number", "number"]);
  const bad = parse('[01, 1., .5, +1, 1e, -, 1.2.3, 0x10]');
  const kinds = bad.root.children[0].children.map((c) => `${c.type}:${c.error}`);
  assert.deepEqual(kinds, [
    "error:bad_number", "error:bad_number", "error:bad_value", "error:bad_value",
    "error:bad_number", "error:bad_number", "error:bad_number", "error:bad_number",
  ]);
  // A wrong number is still a number token (the colour is a number).
  assert.ok(bad.tokens.some((t) => t.kind === "number" && t.end - t.start === 2));
});

test("the literals true, false and null are word tokens and literal nodes", () => {
  const r = parse('[true, false, null, True, nul, nulls, NaN, undefined]');
  assert.deepEqual(r.root.children[0].children.map((c) => c.type), ["literal", "literal", "literal", "error", "error", "error", "error", "error"]);
  assert.deepEqual(r.tokens.filter((t) => t.kind === "word").length, 3);
  assert.equal(r.tokens.filter((t) => t.kind === "other").length, 5);
});

test("comments are tokens of kind comment and error nodes", () => {
  const text = '{\n// one\n"a": /* two */ 1, /* open';
  const r = parse(text);
  const kinds = r.tokens.filter((t) => t.kind === "comment").map((t) => [text.slice(t.start, t.end), t.closed]);
  assert.deepEqual(kinds, [["// one", true], ["/* two */", true], ["/* open", false]]);
  assert.equal(nodesOf(r.root, "error").filter((e) => e.error === "comment").length, 3);
  // The value of the member stays the last child.
  const a = find(r.root, (n) => n.type === "member" && n.name === "a");
  assert.equal(a.children[a.children.length - 1].type, "number");
});

test("wrong characters are other tokens and keep the structure", () => {
  const r = parse("{'a': 1, a: 2, # b: 3}");
  assert.ok(r.tokens.some((t) => t.kind === "other"));
  const names = r.root.children[0].children.filter((m) => m.type === "member").map((m) => `${m.name}|${m.error}`);
  assert.ok(names.includes("a|bad_key"));
  assert.ok(names.includes("#|bad_key"));
  assert.equal(r.tokens.filter((t) => t.kind === "other").map((t) => src(t)).join(""), "'a'a#b");
  function src(t) {
    return "{'a': 1, a: 2, # b: 3}".slice(t.start, t.end);
  }
});

test("a trailing comma, a double comma and a missing comma are faults and the tree goes on", () => {
  const trailing = parse('{"a": [1, 2,], "b": 3,}');
  assert.deepEqual(nodesOf(trailing.root, "error").map((e) => e.error), ["trailing_comma", "trailing_comma"]);
  assert.equal(trailing.root.children[0].children.length, 3);
  const double = parse("[1,,2]");
  assert.deepEqual(nodesOf(double.root, "error").map((e) => e.error), ["unexpected_comma"]);
  assert.equal(double.root.children[0].children.filter((c) => c.type === "number").length, 2);
  const missing = parse('{"a": 1 "b": 2}');
  assert.deepEqual(members(missing.root.children[0]).b.type, "number");
  assert.equal(missing.root.children[0].children[1].error, "missing_comma");
  const arr = parse("[1 2]");
  assert.equal(arr.root.children[0].children[1].error, "missing_comma");
});

test("a member without a colon, without a value or without a key", () => {
  const a = parse('{"a" 1}');
  assert.equal(a.root.children[0].children[0].error, "missing_colon");
  assert.equal(a.root.children[0].children[0].children[0].type, "number");
  const b = parse('{"a": , "b":}');
  assert.deepEqual(a.root.children[0].children.length, 1);
  assert.deepEqual(b.root.children[0].children.map((m) => `${m.name}|${m.error}|${m.children.length}`), ["a|missing_value|0", "b|missing_value|0"]);
  const c = parse('{: 1, "x": 2}');
  assert.equal(c.root.children[0].children[0].error, "missing_key");
  assert.equal(c.root.children[0].children[0].children[0].type, "number");
  const d = parse('{"a"}');
  assert.equal(d.root.children[0].children[0].error, "missing_colon");
});

test("a duplicate member name has an error on the second member", () => {
  const r = parse('{"a": 1, "b": {"a": 1, "a": 2}, "a": 3}');
  const dup = nodesOf(r.root, "member").filter((m) => m.error === "duplicate_member");
  assert.equal(dup.length, 2);
});

test("a closing bracket of the wrong kind closes the nearest frame of its kind", () => {
  const r = parse('{"a": [1, 2}, "b": 3}');
  const top = r.root.children[0];
  assert.equal(top.close, 11);
  const a = members(top).a;
  assert.equal(a.error, "unclosed");
  assert.equal(a.close, undefined);
  assert.equal(a.end, 11);
  const b = parse('[{"a": 1]');
  assert.equal(b.root.children[0].close, 8);
  assert.equal(b.root.children[0].children[0].error, "unclosed");
});

test("a bracket with no partner is an unmatched error node", () => {
  const r = parse('{"a": 1}} ] [');
  const errors = nodesOf(r.root, "error").map((e) => [e.error, e.start]);
  assert.deepEqual(errors, [["unmatched", 8], ["unmatched", 10]]);
  const r2 = parse('{"a": ]}');
  assert.equal(members(r2.root.children[0]).a.type, "error");
  assert.equal(members(r2.root.children[0]).a.error, "unmatched");
});

test("a second value at the top level is flagged, and a text with no value has an empty doc", () => {
  const r = parse('{"a": 1} {"b": 2}');
  assert.equal(r.root.children.length, 2);
  assert.equal(r.root.children[1].error, "extra_value");
  const empty = parse("  \n ");
  assert.equal(empty.root.children.length, 0);
  assert.equal(parse("").tokens.length, 0);
  const stray = parse(", : }");
  assert.deepEqual(stray.root.children.map((c) => c.error), ["unexpected_comma", "unexpected_colon", "unmatched"]);
});

test("a text that is not a string gives a result", () => {
  assert.equal(parseJson(undefined).root.end, 0);
  assert.equal(parseJson(null).tokens.length, 0);
});

test("the options and the text are not changed", () => {
  const variables = ["a"];
  const opts = { variables };
  parse('{"a": "$a"}', opts);
  assert.deepEqual(variables, ["a"]);
});

test("the spans of a text with CRLF and non-ASCII characters are UTF-16 offsets", () => {
  const text = '{\r\n  "k": "é😀",\r\n  "l": [1]\r\n}';
  const r = parse(text);
  const k = members(r.root.children[0]).k;
  assert.equal(text.slice(k.start, k.end), '"é😀"');
  assert.equal(r.root.children[0].foldable, true);
});

// ---- Tolerance ----

test("every prefix of 20 samples gives a result with no exception and the invariants", () => {
  assert.equal(SAMPLES.length, 20);
  for (const text of SAMPLES) {
    for (let k = 0; k <= text.length; k++) {
      const prefix = text.slice(0, k);
      let r;
      assert.doesNotThrow(() => {
        r = parseJson(prefix);
      }, prefix);
      checkInvariants(prefix, r);
    }
  }
});

test("every suffix and every single deletion of the samples give a result with the invariants", () => {
  for (const text of SAMPLES) {
    for (let k = 0; k <= text.length; k++) {
      const suffix = text.slice(k);
      checkInvariants(suffix, parseJson(suffix));
      const cut = text.slice(0, k) + text.slice(k + 1);
      checkInvariants(cut, parseJson(cut));
    }
  }
});

test("random text of the characters of JSON gives a result with the invariants", () => {
  const alphabet = ['{', '}', '[', ']', ':', ',', '"', "\\", "$", "${", "__", "a", "1", "-", ".", "e", " ", "\n", "/", "*", "true", "é", "😀", "u0024"];
  let seed = 12345;
  const rand = () => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed / 0x7fffffff;
  };
  for (let round = 0; round < 3000; round++) {
    let text = "";
    const len = Math.floor(rand() * 40);
    for (let k = 0; k < len; k++) text += alphabet[Math.floor(rand() * alphabet.length)];
    checkInvariants(text, parseJson(text));
    checkInvariants(text, parseJson(text, { variables: ["a"] }));
  }
});

// ---- Coverage ----

test("token coverage: the pieces of the text, with no gap, for the big sample and for a text with every kind", () => {
  const all = '{"a": [1, true, null, "x$y", $z, ${w:csv}, @, /* c */ // d\n], "b\\u0041": "\\u0024q"} \n';
  for (const text of [PIPELINE, all]) {
    const r = parse(text);
    assert.equal(r.tokens.map((t) => text.slice(t.start, t.end)).join(""), text);
  }
  const kinds = new Set(parse(all).tokens.map((t) => t.kind));
  for (const kind of ["blank", "punct", "string", "number", "word", "variable", "other", "comment"]) assert.ok(kinds.has(kind), kind);
  assert.ok(new Set(parse(PIPELINE).tokens.map((t) => t.kind)).has("macro"));
});

test("bracket coverage: each bracket of the code is in a node or in an unmatched error, and none in a string", () => {
  const text = '{"a": "}{][", "b": [{"c": [ ]}], "d": ]} [';
  const r = parse(text);
  const brackets = r.tokens.filter((t) => t.kind === "punct" && "{}[]".includes(text[t.start])).map((t) => t.start);
  const covered = new Set();
  walk(r.root, (n) => {
    if (n.open !== undefined) covered.add(n.open);
    if (n.close !== undefined) covered.add(n.close);
    if (n.type === "error" && n.error === "unmatched") covered.add(n.start);
  });
  assert.deepEqual(brackets.filter((b) => !covered.has(b)), []);
  // The brackets in the string are not tokens of kind punct.
  assert.equal(brackets.includes(text.indexOf("}{][")), false);
});

// ---- Caps ----

test("the depth cap: 128 levels are allowed and 129 give too_deep", () => {
  const ok = "[".repeat(MAX_AST_DEPTH) + "]".repeat(MAX_AST_DEPTH);
  const r = parse(ok);
  assert.equal(r.capped, null);
  const deep = "[".repeat(MAX_AST_DEPTH + 1) + "1" + "]".repeat(MAX_AST_DEPTH + 1);
  const d = parse(deep);
  assert.deepEqual(d.capped, { reason: "too_deep", at: MAX_AST_DEPTH });
  // The tokens go on to the end; the tree stops with the open nodes.
  assert.equal(d.tokens.length, deep.length);
  let levels = 0;
  walk(d.root, (n) => {
    if (n.type === "array") levels += 1;
  });
  assert.equal(levels, MAX_AST_DEPTH);
  // Objects count as levels too.
  const obj = '{"a":'.repeat(MAX_AST_DEPTH + 1);
  assert.equal(parse(obj).capped.reason, "too_deep");
  assert.equal(parse('{"a":'.repeat(MAX_AST_DEPTH) + "1" + "}".repeat(MAX_AST_DEPTH)).capped, null);
});

test("a cap leaves no node that ends after its parent (fuzz crash of f42-props)", () => {
  const texts = ["{".repeat(MAX_AST_DEPTH) + "\n{", "[".repeat(MAX_AST_DEPTH) + "\n[", '{"a":'.repeat(MAX_AST_DEPTH) + "\n{", '{"a":'.repeat(MAX_AST_DEPTH) + '[', "{".repeat(MAX_AST_DEPTH) + '"k" {', "[".repeat(MAX_AST_DEPTH - 1) + '{"a" {'];
  for (const text of texts) {
    const r = parse(text);
    assert.ok(r.capped, text.slice(-8));
  }
  const first = parse("{".repeat(MAX_AST_DEPTH) + "\n{");
  assert.deepEqual(first.capped, { reason: "too_deep", at: MAX_AST_DEPTH + 1 });
  // The node cap in the middle of a member keeps the same rule.
  const cap = `{${new Array(MAX_AST_NODES / 2).fill('"":1').join(",")}}`;
  assert.equal(parse(cap).capped.reason, "too_many_nodes");
});

test("the node cap: the doc and the array count as nodes", () => {
  const make = (count) => `[${new Array(count).fill("1").join(",")}]`;
  const room = MAX_AST_NODES - 2;
  assert.equal(parse(make(room)).capped, null);
  const over = parse(make(room + 1));
  assert.equal(over.capped.reason, "too_many_nodes");
  assert.equal(over.tokens.length, make(room + 1).length);
  let count = 0;
  walk(over.root, () => {
    count += 1;
  });
  assert.equal(count, MAX_AST_NODES);
});

test("the size cap: above 262,144 UTF-8 bytes the tree is empty and the tokens go on", () => {
  const at = MAX_AST_INPUT_BYTES;
  const exact = `"${"a".repeat(at - 2)}"`;
  assert.equal(parse(exact).capped, null);
  const over = parse(`${exact} `);
  assert.deepEqual(over.capped, { reason: "too_large", at });
  assert.equal(over.root.children.length, 0);
  assert.equal(over.tokens.length, 2);
  // The limit counts UTF-8 bytes, not UTF-16 units.
  const bytes = `"${"é".repeat(at / 2 - 1)}"`;
  assert.equal(parse(bytes).capped, null);
  const bytesOver = parse(`"${"é".repeat(at / 2)}"`);
  assert.equal(bytesOver.capped.reason, "too_large");
  assert.ok(bytesOver.capped.at < bytesOver.root.end);
});

// ---- Time ----

/** A pipeline text near the given size, with no cap, made of stages of the kind of a real query. */
function bigPipeline(bytes) {
  const stage = '    {"$match": {"$expr": {"$and": [{"$gte": ["$ts", "$__timeFrom"]}, {"$lt": ["$ts", "$__timeTo"]}]}, "host": "${host:json}", "n": 12.5e3}},\n';
  const parts = ['{"collection": "c", "aggregate": [\n'];
  let size = parts[0].length;
  const tail = '    {"$limit": 1}\n]}';
  while (size + stage.length + tail.length < bytes) {
    parts.push(stage);
    size += stage.length;
  }
  parts.push(tail);
  let text = parts.join("");
  // Fill to the exact size with blanks before the last bracket.
  text = text.slice(0, -2) + " ".repeat(bytes - text.length) + "]}";
  return text;
}

test("time: 262,144 bytes in 100 ms or less (target 50 ms), minimum of 7 runs after 2 warm-ups", () => {
  const text = bigPipeline(MAX_AST_INPUT_BYTES);
  assert.equal(text.length, MAX_AST_INPUT_BYTES);
  const check = parseJson(text);
  assert.equal(check.capped, null, "the sample is under the node cap");
  assert.ok(check.root.children.length > 0);
  // Two runs warm the compiler: the time of the first runs is the time of the engine, not of the reader.
  parseJson(text);
  parseJson(text);
  let best = Infinity;
  for (let k = 0; k < 7; k++) {
    const t0 = performance.now();
    parseJson(text);
    best = Math.min(best, performance.now() - t0);
  }
  console.log(`ast_json time: ${best.toFixed(1)} ms for ${text.length} bytes (${check.tokens.length} tokens, ${nodesOf(check.root, "object").length} objects)`);
  // The target is 50 ms on the reference machine (10.3c). The unit test has a limit of 100 ms for the minimum of 7 runs (16.10a), so a
  // loaded machine does not fail it; the browser case B176 measures the target.
  assert.ok(best <= 100, `minimum of 7: ${best.toFixed(1)} ms`);
});

test("time: the worst texts stay linear (long strings, long blanks, deep brackets, many errors)", () => {
  const cases = [
    `"${"a$".repeat(MAX_AST_INPUT_BYTES / 4)}"`,
    `[${" ".repeat(MAX_AST_INPUT_BYTES - 2)}]`,
    "[".repeat(MAX_AST_INPUT_BYTES / 2),
    "}".repeat(MAX_AST_INPUT_BYTES / 2),
    "@".repeat(MAX_AST_INPUT_BYTES / 2),
    ",:".repeat(MAX_AST_INPUT_BYTES / 4),
    `"${"\\u0024a".repeat(MAX_AST_INPUT_BYTES / 8)}"`,
  ];
  for (const text of cases) {
    const t0 = performance.now();
    const r = parseJson(text);
    const ms = performance.now() - t0;
    assert.equal(r.tokens.length > 0, true);
    assert.ok(ms < 500, `${ms.toFixed(0)} ms`);
  }
});
