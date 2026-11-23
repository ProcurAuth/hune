// The sort of the rows of a result. A numeric column is sorted as numbers. The text of a Decimal or of a big integer is compared as a number, and NaN goes last.
// A text column is sorted as text.
// hune: E21. The sort of the rows and its WebAssembly kernel are removed: Hune imports only compareNumericForSort and
// compareTextForSort (engine-patches.md). The imports app_loader and app_wasm go with them.

import { NUMERIC_RE } from "./app_results_values.js";

// The text of a Float value is sent by the API as a string (docs/telemetry.md, "Result values"). The infinities sort as numbers.
// NaN sorts with the NULLs, last.
const FLOAT_WORDS = new Map([["inf", Infinity], ["+inf", Infinity], ["-inf", -Infinity], ["nan", NaN], ["-nan", NaN], ["+nan", NaN]]);
// hune: E29. String steps replace the pattern /^([+-]?)0*(\d*?)(?:\.(\d*?)0*)?$/, whose time grew with the square of the length.
function decimalParts(text) {
  if (!/^[+-]?\d*(?:\.\d*)?$/.test(text)) return null;
  const sign = /^[+-]/.test(text) ? text[0] : "";
  const [int, frac] = text.slice(sign.length).split(".");
  let j = frac ? frac.length : 0;
  while (j && frac[j - 1] === "0") j--;
  return [text, sign, int.replace(/^0+/, ""), frac === undefined ? frac : frac.slice(0, j)];
}

// A double cannot hold these digits. A Decimal(38, 9) text is compared as text (compareDecimalText).
function longDecimalText(text) {
  if (text.indexOf(".") < 0 || !NUMERIC_RE.test(text) || /[eE]/.test(text)) return false;
  return text.replace(/[^0-9]/g, "").replace(/^0+/, "").length > 15;
}

// Two decimal texts (digits, one dot, a sign) are compared exactly.
// hune: E29. `ma` and `mb` can come from the normalised values (member `p`).
function compareDecimalText(a, b, ma = decimalParts(a), mb = decimalParts(b)) {
  if (!ma || !mb) return a < b ? -1 : a > b ? 1 : 0;
  const zero = (m) => !m[2] && !m[3];
  const sa = ma[1] === "-" && !zero(ma) ? -1 : 1;
  const sb = mb[1] === "-" && !zero(mb) ? -1 : 1;
  if (sa !== sb) return sa < sb ? -1 : 1;
  let cmp = 0;
  if (ma[2].length !== mb[2].length) cmp = ma[2].length < mb[2].length ? -1 : 1;
  else if (ma[2] !== mb[2]) cmp = ma[2] < mb[2] ? -1 : 1;
  else {
    const fa = ma[3] || "";
    const fb = mb[3] || "";
    cmp = fa < fb ? -1 : fa > fb ? 1 : 0;
  }
  return sa < 0 ? -cmp : cmp;
}

function normalizeNumericForSort(raw) {
  if (raw === null || raw === undefined) return { t: "z", v: null };
  if (typeof raw === "number") {
    if (!Number.isFinite(raw)) return { t: "z", v: null };
    return { t: "n", v: raw };
  }
  const s = typeof raw === "string" ? raw.trim() : String(raw).trim();
  if (!s) return { t: "z", v: null };
  const word = FLOAT_WORDS.get(s.toLowerCase());
  if (word !== undefined) return Number.isNaN(word) ? { t: "z", v: null } : { t: "n", v: word };
  if (longDecimalText(s)) return { t: "d", v: s, p: decimalParts(s) };
  if (/^[+-]?\d+$/.test(s)) {
    try {
      return { t: "bi", v: BigInt(s) };
    } catch {
      return { t: "s", v: s.toLowerCase() };
    }
  }
  if (NUMERIC_RE.test(s)) {
    const n = Number(s);
    if (Number.isFinite(n)) return { t: "n", v: n };
  }
  return { t: "s", v: s.toLowerCase() };
}

function compareNumericForSort(a, b) {
  return compareNormalizedForSort(normalizeNumericForSort(a), normalizeNumericForSort(b));
}

// hune: E29. A sort normalises each row once, then compares the normalised values.
function compareNormalizedForSort(na, nb) {
  if (na.t === "z" && nb.t === "z") return 0;
  if (na.t === "z") return 1;
  if (nb.t === "z") return -1;

  if (na.t === "bi" && nb.t === "bi") return na.v < nb.v ? -1 : na.v > nb.v ? 1 : 0;

  if (na.t === "d" || nb.t === "d") {
    if (na.t !== "s" && nb.t !== "s") {
      const text = (x) => (x.t === "bi" ? x.v.toString() : x.t === "n" && Number.isFinite(x.v) && !/e/i.test(String(x.v)) ? String(x.v) : null);
      const ta = na.t === "d" ? na.v : text(na);
      const tb = nb.t === "d" ? nb.v : text(nb);
      if (ta !== null && tb !== null) return compareDecimalText(ta, tb, na.p, nb.p);
      const an = Number(na.v);
      const bn = Number(nb.v);
      return an < bn ? -1 : an > bn ? 1 : 0;
    }
  }

  if (na.t === "n" && nb.t === "n") return na.v < nb.v ? -1 : na.v > nb.v ? 1 : 0;

  if (na.t === "bi" && nb.t === "n") {
    if (Number.isSafeInteger(nb.v)) {
      const bbi = BigInt(nb.v);
      return na.v < bbi ? -1 : na.v > bbi ? 1 : 0;
    }
    const an = Number(na.v);
    if (Number.isFinite(an)) return an < nb.v ? -1 : an > nb.v ? 1 : 0;
  }

  if (na.t === "n" && nb.t === "bi") {
    if (Number.isSafeInteger(na.v)) {
      const abi = BigInt(na.v);
      return abi < nb.v ? -1 : abi > nb.v ? 1 : 0;
    }
    const bn = Number(nb.v);
    if (Number.isFinite(bn)) return na.v < bn ? -1 : na.v > bn ? 1 : 0;
  }

  const sa = na.t === "bi" ? na.v.toString() : String(na.v);
  const sb = nb.t === "bi" ? nb.v.toString() : String(nb.v);

  if (sa.length !== sb.length) return sa.length < sb.length ? -1 : 1;
  if (sa < sb) return -1;
  if (sa > sb) return 1;
  return 0;
}

function compareTextForSort(a, b) {
  const sa = String(a ?? "");
  const sb = String(b ?? "");
  const la = sa.toLowerCase();
  const lb = sb.toLowerCase();
  if (la < lb) return -1;
  if (la > lb) return 1;
  if (sa < sb) return -1;
  if (sa > sb) return 1;
  return 0;
}

// hune: E21, the sort of the rows is removed. E29 exports the two steps of compareNumericForSort.
export { compareNormalizedForSort, compareNumericForSort, compareTextForSort, normalizeNumericForSort };
