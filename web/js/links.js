// Data links (9.18, FE-09). Pure module: no DOM access.

const TOKEN_RE = /\$\{([^}]*)\}|\$([A-Za-z_][A-Za-z0-9_]*)/g;

/**
 * Check the final URL (9.18, 4.23): it starts with `/`, `?`, `#`, `http://` or `https://`, and not with `//` or `/\`.
 * The scheme compares with ASCII letters folded to lower case.
 */
export function checkUrl(url) {
  if (typeof url !== "string" || url === "") return false;
  if (url.startsWith("//") || url.startsWith("/\\")) return false;
  if (url[0] === "/" || url[0] === "?" || url[0] === "#") return true;
  const head = url.slice(0, 8).replace(/[A-Z]/g, (c) => c.toLowerCase());
  return head.startsWith("http://") || head.startsWith("https://");
}

const enc = encodeURIComponent;
// The encodings of Grafana 12.4.2 (9.18): `uri` is the format `uriencode`, `param` the form of `urlUtil.toUrlParams`.
const hex = (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`;
const uri = (s) => encodeURI(s).replace(/[!'()*]/g, hex);
const strict = (s) => enc(s).replace(/[!'()*]/g, hex);
const param = (s) => strict(s).replace(/%(40|3A|24|2C|3B)/g, decodeURIComponent);
// Whole milliseconds, as Grafana gives a time. The round to 1 µs removes the float error of frame.js.
const wholeMs = (v) => Math.floor(Math.round(v * 1000) / 1000);

/** The number of `${__value.numeric}` (9.18). */
export function numericOf(field, v) {
  const unit = field.config && field.config.unit;
  if (v === null || v === "" || unit === "string" || (field.kind === "text" && !unit)) return NaN;
  return field.kind === "time" ? wholeMs(v) : Number(v);
}

function variableText(variable) {
  return (variable.values || []).join(",");
}

/** The text `var-<name>=<value>` of each variable, joined with `&`, in the order of the list (the token `${__all_variables}`, 9.18). */
export function allVariables(variables) {
  const parts = [];
  for (const v of variables || []) {
    if (v.all) parts.push(`var-${enc(v.name)}=${enc("$__all")}`);
    else for (const value of v.values || []) parts.push(`var-${enc(v.name)}=${enc(value)}`);
  }
  return parts.join("&");
}

/** The text of `${<name>:queryparam}` (9.18). */
export function queryParam(v) {
  const e = v.kind === "constant" ? strict : param;
  return (v.all ? ["$__all"] : v.values && v.values.length ? v.values : [""]).map((x) => `var-${v.name}=${e(x)}`).join("&");
}

/**
 * Replace the tokens of a URL template.
 * ctx: { rawValue, displayText, fieldName, rowValue(name) -> text | undefined, from, to, variables: [{name, kind, values, all}], interpolate(text),
 * valueTime, valueNumeric, seriesName, timezone, dashboardUid }
 */
export function expandUrl(template, ctx) {
  return String(template).replace(TOKEN_RE, (whole, braced, bare) => {
    const name = braced !== undefined ? braced.trim() : bare;
    switch (name) {
      case "__value.raw":
        return ctx.rawValue === undefined ? whole : enc(String(ctx.rawValue));
      case "__value.text":
        return ctx.displayText === undefined ? whole : enc(ctx.displayText);
      case "__value.time":
        return ctx.valueTime === undefined ? whole : typeof ctx.valueTime === "number" ? uri(String(wholeMs(ctx.valueTime))) : "";
      case "__value.numeric":
        return ctx.valueNumeric === undefined ? whole : uri(String(ctx.valueNumeric));
      case "__series.name":
        return ctx.seriesName === undefined ? whole : uri(ctx.seriesName);
      case "__timezone":
        return ctx.timezone === undefined ? whole : uri(ctx.timezone === "browser" ? Intl.DateTimeFormat().resolvedOptions().timeZone : ctx.timezone);
      case "__dashboard.uid":
        return ctx.dashboardUid === undefined ? whole : uri(ctx.dashboardUid);
      case "__field.name":
        return ctx.fieldName === undefined ? whole : enc(ctx.fieldName);
      case "__from":
        return ctx.from === undefined ? whole : enc(String(ctx.from));
      case "__to":
        return ctx.to === undefined ? whole : enc(String(ctx.to));
      case "__url_time_range":
        return `from=${ctx.from}&to=${ctx.to}`;
      case "__all_variables":
        return allVariables(ctx.variables);
      default:
        break;
    }
    if (braced !== undefined) {
      const dotted = /^__data\.fields\.([^\s"\[\]]+)$/.exec(name);
      const quoted = /^__data\.fields\["(.*)"\]$/.exec(name);
      const fieldName = dotted ? dotted[1] : quoted ? quoted[1] : null;
      if (fieldName !== null) {
        const text = ctx.rowValue ? ctx.rowValue(fieldName) : undefined;
        return text === undefined ? whole : enc(text);
      }
    }
    const asParam = /^(\w+):queryparam$/.exec(name);
    const variable = (ctx.variables || []).find((v) => v.name === (asParam ? asParam[1] : name));
    if (variable) return asParam ? queryParam(variable) : enc(variable.all ? "$__all" : variableText(variable));
    return whole;
  });
}

/**
 * Build a link of a data link or a panel link. `href` is null when the final URL is not permitted (the caller shows
 * plain text and writes a warning).
 */
export function buildLink(link, ctx) {
  const url = expandUrl(link.url, ctx);
  const ok = checkUrl(url);
  // The title shows after the replacement of the variables (9.13, F13). The caller writes it as text.
  const title = typeof ctx.interpolate === "function" ? ctx.interpolate(link.title) : link.title;
  return { title, href: ok ? url : null, target_blank: link.target_blank === true };
}
