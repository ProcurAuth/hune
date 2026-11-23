// Value mappings (9.17, FE-08). Pure module: no DOM access.
import { parseGrafanaPattern, regexExec, regexTest } from "./fields.js";

function matches(mapping, v, rawText) {
  switch (mapping.kind) {
    case "value":
      return rawText === mapping.value;
    case "range": {
      if (typeof v !== "number") return false;
      if (mapping.from !== null && mapping.from !== undefined && !(mapping.from <= v)) return false;
      if (mapping.to !== null && mapping.to !== undefined && !(v <= mapping.to)) return false;
      return true;
    }
    case "regex": {
      // A raw text longer than MAX_PATTERN_TEXT never matches (9.14, T87).
      const re = parseGrafanaPattern(mapping.pattern);
      return re !== null && regexTest(re, rawText);
    }
    case "special":
      switch (mapping.match) {
        case "null": return v === null;
        case "nan": return typeof v === "number" && Number.isNaN(v);
        case "null_nan": return v === null || (typeof v === "number" && Number.isNaN(v));
        case "true": return v === true;
        case "false": return v === false;
        case "empty": return v === "";
        default: return false;
      }
    default:
      return false;
  }
}

/**
 * Apply the mappings to a Grafana value. The first mapping that matches gives the result.
 * Returns null, or {text, color} where each member can be null (no change).
 */
export function applyMappings(mappings, value) {
  if (!mappings || mappings.length === 0) return null;
  const rawText = String(value);
  for (const m of mappings) {
    if (!matches(m, value, rawText)) continue;
    let text = m.text === undefined ? null : m.text;
    if (m.kind === "regex" && text !== null) {
      // The group references $1 to $9 take their group of the first match, as String.prototype.replace does.
      const found = regexExec(parseGrafanaPattern(m.pattern), rawText);
      text = text.replace(/\$([1-9])/g, (whole, n) => (found && found[Number(n)] !== undefined ? found[Number(n)] : ""));
    }
    return { text, color: m.color === undefined ? null : m.color };
  }
  return null;
}
