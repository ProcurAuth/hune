// Helpers of the front-end tests: fields from compact column specs.
import { framesToFields } from "../../web/js/frame.js";
import { Runtime } from "../../web/js/runtime.js";
import { variableNamesOfLines } from "../../web/js/sqlscan.js";

// cols: [{name, kind, values, type?}] in the FRAME form of 7.16 (time in Unix seconds).
export function frameOf(cols) {
  return {
    columns: cols.map((c) => ({ name: c.name, kind: c.kind, type: c.type || c.kind })),
    values: cols.map((c) => c.values),
    rows: cols[0] ? cols[0].values.length : 0,
  };
}

export function toFields(cols) {
  return framesToFields(frameOf(cols));
}

// A plain array view of a field, for assertions: Grafana-like values (typed arrays become arrays).
export function plain(field) {
  if (field.kind === "number" || field.kind === "time") return Array.from(field.values, (v, i) => (field.nulls[i] ? null : v));
  if (field.kind === "number_text") return field.values.slice();
  return Array.from(field.values);
}

// The member `names` of the answer of 6.14 for a document (7.19, 9.10), made with the scan of the editor: the same shape as the server gives, so a
// test can build a runtime. The server makes this member in the real page.
export function namesOfDocument(doc) {
  const known = new Set(doc.variables.map((v) => v.name));
  const list = (lines, skip) => variableNamesOfLines(lines).filter((n) => known.has(n) && n !== skip);
  const panels = {};
  for (const section of doc.sections) {
    for (const panel of section.panels) {
      const data = panel.data;
      if (data && data.query) panels[String(panel.id)] = [list(data.query.sql), ...(data.extra_queries || []).map((q) => list(q.sql))];
    }
  }
  const variables = {};
  for (const v of doc.variables) if (v.kind === "query") variables[v.name] = list(v.sql, v.name);
  return { panels, variables, annotations: (doc.annotations || []).map((a) => list(a.sql)) };
}

// The runtime with the member `names` made from the document, unless the test gives `names` (undefined included).
export class RuntimeWithNames extends Runtime {
  constructor(options) {
    super({ ...options, names: "names" in options ? options.names : namesOfDocument(options.doc) });
  }
}
