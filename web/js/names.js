// Display names (9.13, FE-04). Pure module: no DOM access.

function hasLabels(f) {
  return !!f.labels && Object.keys(f.labels).length > 0;
}

/** The base display name of each field of `fields` (steps 1 to 5 of 9.13). */
export function baseDisplayNames(fields) {
  const labelled = fields.filter(hasLabels);
  // Step 3 condition: all labelled fields have exactly one key, the same key K.
  let commonKey = null;
  if (labelled.length > 0) {
    const first = Object.keys(labelled[0].labels);
    if (first.length === 1 && labelled.every((f) => {
      const keys = Object.keys(f.labels);
      return keys.length === 1 && keys[0] === first[0];
    })) {
      commonKey = first[0];
    }
  }
  const base = fields.map((f) => {
    if (f.kind === "time" && !hasLabels(f)) return f.name !== "" ? f.name : "Time";
    // Step 1a: a field that has a prefix starts with it. The prefix is the ref of its query (9.22a, rule 5).
    const parts = f.prefix ? [f.prefix] : [];
    if (f.name !== "" && f.name !== "Value") parts.push(f.name);
    if (hasLabels(f)) {
      if (commonKey !== null) {
        const v = f.labels[commonKey];
        if (v !== undefined && v !== null && v !== "") parts.push(String(v));
      } else {
        const keys = Object.keys(f.labels).sort();
        parts.push(`{${keys.map((k) => `${k}="${f.labels[k]}"`).join(", ")}}`);
      }
    }
    if (parts.length === 0) return f.name !== "" ? f.name : "Value";
    return parts.join(" ");
  });
  // Step 5: equal names get a counter, in column order. A prefix is part of the name here.
  const own = fields.map((f) => (f.prefix ? `${f.prefix} ${f.name}` : f.name));
  const counts = new Map();
  for (const n of own) counts.set(n, (counts.get(n) || 0) + 1);
  const seen = new Map();
  return base.map((b, i) => {
    if (b === own[i] && counts.get(own[i]) > 1) {
      const k = (seen.get(own[i]) || 0) + 1;
      seen.set(own[i], k);
      return `${b} ${k}`;
    }
    return b;
  });
}

/** Grafana `formatLabels`: `{k1="v1", k2="v2"}`, the keys in code unit order. */
function formatLabels(labels) {
  const keys = Object.keys(labels).sort();
  return `{${keys.map((k) => `${k}="${labels[k]}"`).join(", ")}}`;
}

// The Grafana template pattern: $name, [[name:format]], ${name.path:format}.
const TEMPLATE = /\$(\w+)|\[\[([\s\S]+?)(?::(\w+))?\]\]|\$\{(\w+)(?:\.([^:}]+))?(?::([^}]+))?\}/g;

/** The value of the path after `__field` for one field, or undefined when the path names no property. */
function fieldValue(field, base, path) {
  if (path === "name") return field.name;
  if (path === "displayName") return base;
  const labels = hasLabels(field) ? field.labels : null;
  if (path === "labels" || path === "formattedLabels") return labels ? formatLabels(labels) : "";
  if (path.startsWith("labels.")) {
    const key = path.slice(7);
    const v = labels ? labels[key] : undefined;
    return v === undefined || v === null ? "" : String(v);
  }
  return undefined;
}

/**
 * Interpolate a Grafana display-name template for one field. The result is text, never HTML.
 * Known: `${__field.name}`, `${__field.displayName}` (the base display name), `${__field.labels.<key>}`,
 * `${__field.labels}` and `${__series.name}` (the optional `seriesName`, else empty). A label that the field lacks
 * gives the empty text. Any other name stays as written, as Grafana leaves an unknown variable. A format suffix
 * (`:text`) is read and ignored.
 */
export function interpolateDisplayName(template, field, base, seriesName = "") {
  if (typeof template !== "string" || !template.includes("$") && !template.includes("[[")) return template;
  return template.replace(TEMPLATE, (match, bare, bracket, _bracketFormat, braceName, braceFirstPath) => {
    let name = bare;
    let path = "";
    if (braceName !== undefined) {
      name = braceName;
      path = braceFirstPath || "";
    } else if (bracket !== undefined) {
      // [[__field.labels.k]] holds the dotted path in one piece.
      const dot = bracket.indexOf(".");
      name = dot < 0 ? bracket : bracket.slice(0, dot);
      path = dot < 0 ? "" : bracket.slice(dot + 1);
    }
    if (name === "__field") {
      if (bare !== undefined) return match;
      const v = fieldValue(field, base, path);
      return v === undefined ? "" : v;
    }
    if (name === "__series" && bare === undefined) {
      return path === "name" ? String(seriesName) : "";
    }
    return match;
  });
}

/**
 * The display name of each field: `config.display_name` (interpolated, 9.13) when set and not empty after the
 * interpolation, else the base display name. `options.seriesName` is the value of `${__series.name}`.
 */
export function displayNames(fields, options = {}) {
  const base = baseDisplayNames(fields);
  const seriesName = options.seriesName === undefined ? "" : options.seriesName;
  return fields.map((f, i) => {
    const d = f.config && f.config.display_name;
    if (d === undefined || d === null || d === "") return base[i];
    const text = interpolateDisplayName(String(d), f, base[i], seriesName);
    return text !== "" ? text : base[i];
  });
}
