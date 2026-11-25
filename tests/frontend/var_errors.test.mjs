// SPDX-License-Identifier: MIT OR Apache-2.0
// The readable text of a failed options request (9.10): the code of the API first, the ClickHouse number second, the text third.
import test from "node:test";
import assert from "node:assert/strict";
import { GENERAL_OPTIONS_ERROR, clickhouseCodeOf, optionsErrorInfo } from "../../web/js/var_errors.js";

const err = (code, message, extra = {}, retryable = false) => ({ code, message, extra, retryable });

// The real text of the failure of the variable "Metric service" (ClickHouse 26.3, 7 days of data, a limit of 256 MiB).
const MEMORY_TEXT =
  "Code: 241. DB::Exception: Query memory limit exceeded: would use 262.51 MiB (attempt to allocate chunk of 4.08 MiB), maximum: 256.00 MiB: (while reading column TimeUnix): (MEMORY_LIMIT_EXCEEDED) (version 26.3.17.56 (official build))";

test("the code query_memory_limit of the API gives the short sentence of the memory", () => {
  const info = optionsErrorInfo(err("query_memory_limit", MEMORY_TEXT));
  assert.equal(info.short, "The query used too much memory.");
  assert.equal(info.detail, MEMORY_TEXT, "the full text stays whole");
  assert.equal(info.code, "query_memory_limit");
});

test("the real text of the ClickHouse error 241 gives the memory sentence also under a code of the API that the table does not know", () => {
  assert.equal(optionsErrorInfo(err("something_new", MEMORY_TEXT)).short, "The query used too much memory.");
  assert.equal(clickhouseCodeOf(err("query_error", MEMORY_TEXT)), 241);
});

test("a query_error uses the number of the member clickhouse_code, then the number of the text, then the name of the text", () => {
  assert.equal(optionsErrorInfo(err("query_error", "x", { clickhouse_code: 62 })).short, "The query has a syntax error.");
  assert.equal(optionsErrorInfo(err("query_error", "Code: 60. DB::Exception: Table demo.nope does not exist. (UNKNOWN_TABLE) (version 26.3.1.1 (official build))")).short, "The query names a table that does not exist.");
  // No number in the text: the name, in the last parentheses before the version.
  assert.equal(optionsErrorInfo(err("query_error", "DB::Exception: no such column (UNKNOWN_IDENTIFIER) (version 26.3.1.1 (official build))")).short, "The query names a column that does not exist.");
  // The member wins over the text.
  assert.equal(optionsErrorInfo(err("query_error", "Code: 60. x (UNKNOWN_TABLE)", { clickhouse_code: 47 })).short, "The query names a column that does not exist.");
});

test("the closed list of the common ClickHouse errors", () => {
  const sentence = (n) => optionsErrorInfo(err("query_error", "x", { clickhouse_code: n })).short;
  assert.equal(sentence(47), "The query names a column that does not exist.");
  assert.equal(sentence(60), "The query names a table that does not exist.");
  assert.equal(sentence(62), "The query has a syntax error.");
  assert.equal(sentence(159), "The query took too long.");
  assert.equal(sentence(202), "ClickHouse has too many queries at this time.");
  assert.equal(sentence(209), "ClickHouse did not answer in time.");
  assert.equal(sentence(210), "Hune could not reach ClickHouse.");
  assert.equal(sentence(241), "The query used too much memory.");
  assert.equal(sentence(497), "The account of Hune has no right for this query.");
  assert.equal(sentence(516), "ClickHouse refused the login of Hune.");
});

test("a query_error with a number that the list does not know says that the source refused the query", () => {
  assert.equal(optionsErrorInfo(err("query_error", "Code: 999. DB::Exception: odd (SOMETHING_ELSE)", { clickhouse_code: 999 })).short, "The source refused the query.");
});

test("the codes of the API that name the cause have their own sentence", () => {
  assert.equal(optionsErrorInfo(err("query_timeout", "t")).short, "The query took too long.");
  assert.equal(optionsErrorInfo(err("source_unavailable", "t", {}, true)).short, "Hune cannot reach the source.");
  assert.equal(optionsErrorInfo(err("source_busy", "t", {}, true)).short, "The source is busy.");
  assert.equal(optionsErrorInfo(err("source_access_denied", "t")).short, "The account of Hune has no right for this query.");
  assert.equal(optionsErrorInfo(err("forbidden", "t")).short, "You may not read these options.");
  assert.equal(optionsErrorInfo(err("network_error", "Hune does not answer.", {}, true)).short, "Hune does not answer.");
  assert.equal(optionsErrorInfo(err("request_timeout", "t")).short, "Hune did not answer in time.");
});

test("an error with no known cause gets the general sentence, and the retryable flag follows the error", () => {
  const info = optionsErrorInfo(err("weird", "nothing useful", {}, true));
  assert.equal(info.short, GENERAL_OPTIONS_ERROR);
  assert.equal(info.retryable, true);
  assert.equal(optionsErrorInfo(null).short, GENERAL_OPTIONS_ERROR);
  assert.equal(optionsErrorInfo({}).detail, "");
});

test("a text with a name that is not in the list gives no number", () => {
  assert.equal(clickhouseCodeOf(err("query_error", "DB::Exception: odd (SOMETHING_ELSE)")), null);
  assert.equal(clickhouseCodeOf(err("query_error", "(UNKNOWN_TABLE) in the middle (and text after)")), null, "the name must be at the end");
});
