// The readable text of a failed options request of a variable (9.10, rule "options error"). Pure module: no DOM access.
// The control shows a short sentence, a button Retry and, folded, the full text of the source. The short sentence comes from the first rule that fits:
//   1. the code of the API error (docs/03, 6.8): `query_memory_limit`, `query_timeout`, `source_unavailable` ...
//   2. the member `clickhouse_code` of a `query_error` (a ClickHouse error number, for example 47 or 62),
//   3. the text of the message: it starts with `Code: <number>.` and ends with `(<NAME>)` before `(version ...)`: the number first, the name second,
//   4. a general sentence.
// The page never changes what the source said: the full text stays in `detail`.

/** The short sentences by code of the API. */
const BY_API_CODE = new Map([
  ["query_memory_limit", "The query used too much memory."],
  ["query_timeout", "The query took too long."],
  ["query_limit", "The query stopped at a limit of the source."],
  ["sql_invalid", "The SQL of the variable is not valid."],
  ["sql_too_large", "The SQL of the variable is too large."],
  ["source_access_denied", "The account of Hune has no right for this query."],
  ["source_misconfigured", "The login or the certificate for the source failed."],
  ["source_unavailable", "Hune cannot reach the source."],
  ["source_busy", "The source is busy."],
  ["source_bad_response", "The source gave an answer that Hune cannot read."],
  ["server_busy", "Hune is busy."],
  ["pdp_unavailable", "The access check did not answer."],
  ["forbidden", "You may not read these options."],
  ["not_found", "The variable or its source does not exist."],
  ["network_error", "Hune does not answer."],
  ["request_timeout", "Hune did not answer in time."],
  ["internal_error", "Hune had a fault."],
]);

/** The short sentences by ClickHouse error number (the closed list of the common errors). */
const BY_CLICKHOUSE_CODE = new Map([
  [47, "The query names a column that does not exist."],
  [60, "The query names a table that does not exist."],
  [62, "The query has a syntax error."],
  [159, "The query took too long."],
  [202, "ClickHouse has too many queries at this time."],
  [209, "ClickHouse did not answer in time."],
  [210, "Hune could not reach ClickHouse."],
  [241, "The query used too much memory."],
  [497, "The account of Hune has no right for this query."],
  [516, "ClickHouse refused the login of Hune."],
]);

/** The same errors by name, for a text that has no number. */
const BY_CLICKHOUSE_NAME = new Map([
  ["UNKNOWN_IDENTIFIER", 47],
  ["UNKNOWN_TABLE", 60],
  ["SYNTAX_ERROR", 62],
  ["TIMEOUT_EXCEEDED", 159],
  ["TOO_MANY_SIMULTANEOUS_QUERIES", 202],
  ["SOCKET_TIMEOUT", 209],
  ["NETWORK_ERROR", 210],
  ["MEMORY_LIMIT_EXCEEDED", 241],
  ["ACCESS_DENIED", 497],
  ["AUTHENTICATION_FAILED", 516],
]);

export const GENERAL_OPTIONS_ERROR = "The options did not load.";

const NUMBER_IN_TEXT = /^\s*Code:\s*(\d+)\./;
// The name of the error: the last upper-case word in parentheses, before an optional `(version ...)`.
const NAME_IN_TEXT = /\(([A-Z][A-Z0-9_]{2,})\)(?:\s*\(version .*\))?\s*$/;

/** The ClickHouse number of an error: the member `clickhouse_code`, else the number of the text, else the number of the name in the text. */
export function clickhouseCodeOf(error) {
  const member = error && error.extra ? error.extra.clickhouse_code : undefined;
  if (Number.isInteger(member)) return member;
  const text = error && typeof error.message === "string" ? error.message : "";
  const number = NUMBER_IN_TEXT.exec(text);
  if (number) return Number(number[1]);
  const name = NAME_IN_TEXT.exec(text);
  return name && BY_CLICKHOUSE_NAME.has(name[1]) ? BY_CLICKHOUSE_NAME.get(name[1]) : null;
}

/**
 * What the control shows for a failed options request: { short, detail, code, retryable }.
 * `short` is one sentence. `detail` is the full text of the error, for the folded part. `code` is the code of the API.
 */
export function optionsErrorInfo(error) {
  const code = error && typeof error.code === "string" ? error.code : "";
  const detail = error && typeof error.message === "string" ? error.message : "";
  let short = BY_API_CODE.get(code);
  // A `query_error` has no sentence of its own: the number of ClickHouse tells more. A code that the API does not know does the same.
  if (!short || code === "query_error") {
    const number = clickhouseCodeOf(error);
    const own = code === "query_error" ? "The source refused the query." : short;
    short = (number !== null && BY_CLICKHOUSE_CODE.get(number)) || own || GENERAL_OPTIONS_ERROR;
  }
  return { short, detail, code, retryable: !!(error && error.retryable) };
}
