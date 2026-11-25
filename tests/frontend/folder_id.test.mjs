// SPDX-License-Identifier: MIT OR Apache-2.0
// Tests of the id of a new folder (10.7): the page makes it from the title, and it follows the rule of the API.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { ID_RULE, folderIdFromTitle, folderIdWithSuffix } from "../../web/js/editor/folder_id.js";

const read = (p) => readFileSync(new URL(`../../web/${p}`, import.meta.url), "utf8");

// A fill that gives the bytes 0, 1, 2, 3: the suffix is "0123". It makes the tests the same on each run.
const fill = (bytes) => {
  bytes.forEach((_, i) => { bytes[i] = i; });
  return bytes;
};

test("the id follows the rule of the API for a plain title", () => {
  assert.equal(folderIdFromTitle("Operations", fill), "operations");
  assert.equal(folderIdFromTitle("Start here", fill), "start-here");
  assert.equal(folderIdFromTitle("  Edge & Telemetry  ", fill), "edge-telemetry");
  assert.equal(folderIdFromTitle("a--b", fill), "a-b");
  for (const title of ["Operations", "Start here", "Edge & Telemetry", "2026 plan"]) assert.match(folderIdFromTitle(title, fill), ID_RULE, title);
});

test("accents are removed and other characters become one dash", () => {
  assert.equal(folderIdFromTitle("Équipe réseau", fill), "equipe-reseau");
  assert.equal(folderIdFromTitle("Crème brûlée", fill), "creme-brulee");
  assert.equal(folderIdFromTitle("a / b \\ c", fill), "a-b-c");
  assert.equal(folderIdFromTitle("日本 Tokyo", fill), "tokyo", "a letter that is not ASCII gives no character");
});

test("a title with no letter and no digit gets a short suffix, so the id is never empty", () => {
  assert.equal(folderIdFromTitle("***", fill), "folder-0123");
  assert.equal(folderIdFromTitle("日本語", fill), "folder-0123");
  assert.equal(folderIdFromTitle("", fill), "folder-0123");
  assert.match(folderIdFromTitle("!!!", fill), ID_RULE);
});

test("the id has 40 characters at most and never ends with a dash", () => {
  const id = folderIdFromTitle("a".repeat(100), fill);
  assert.equal(id.length, 40);
  assert.match(id, ID_RULE);
  // The cut falls on a dash: the id loses the dash at its end.
  const cut = folderIdFromTitle(`${"a".repeat(39)} b`, fill);
  assert.equal(cut, "a".repeat(39));
  assert.ok(!cut.endsWith("-"));
  // A title that starts with symbols starts with a letter or a digit.
  assert.equal(folderIdFromTitle("--- Ops", fill), "ops");
});

test("a suffix keeps the rule, also for the longest id", () => {
  const short = folderIdWithSuffix("ops", fill);
  assert.equal(short, "ops-0123");
  assert.match(short, ID_RULE);
  const long = folderIdWithSuffix("a".repeat(40), fill);
  assert.equal(long, `${"a".repeat(35)}-0123`);
  assert.equal(long.length, 40);
  assert.match(long, ID_RULE);
  // The cut does not leave two dashes.
  const dashed = folderIdWithSuffix(`${"a".repeat(34)}-bbbbb`, fill);
  assert.ok(!dashed.includes("--"), dashed);
  assert.match(dashed, ID_RULE);
});

test("the suffix of a real fill uses lower-case letters and digits only", () => {
  const real = (bytes) => { for (let i = 0; i < bytes.length; i++) bytes[i] = 255 - i * 37; return bytes; };
  for (const id of [folderIdFromTitle("###", real), folderIdWithSuffix("ops", real)]) assert.match(id, ID_RULE);
});

test("the dialog New folder has the field Title only and tries again with a suffix when the id exists", () => {
  const js = read("js/editor/folders.js");
  const dialog = js.slice(js.indexOf("export function newFolder"), js.indexOf("export function renameFolder"));
  assert.ok(!/"Id"|folder-id|ID_RULE/.test(dialog), "no field Id, no rule for the person to follow");
  assert.match(dialog, /folderIdFromTitle\(t, fill\)/);
  assert.match(dialog, /attempt < 3/, "three tries");
  assert.match(dialog, /error\.code !== "already_exists" \|\| attempt === 2\) return errorText\(error\)/, "another error, or the last try, shows its error");
  assert.match(dialog, /id = folderIdWithSuffix\(id, fill\)/);
  assert.match(dialog, /testid: "folder-title"/);
});

test("the dialog Import has the entry New folder, only when the person can create a folder, and makes the folder with the same id rule", () => {
  const js = read("js/editor/import_export.js");
  assert.match(js, /const NEW_FOLDER = "::new-folder::";/);
  assert.ok(!ID_RULE.test(read("js/editor/import_export.js").match(/NEW_FOLDER = "([^"]+)"/)[1]), "the value of the entry is never a folder id");
  assert.match(js, /const canCreateFolder = ctx\.canCreateFolder === true;/);
  assert.match(js, /\.\.\.\(canCreateFolder \? \[el\("option", \{ value: NEW_FOLDER, text: "New folder\.\.\." \}\)\] : \[\]\)/);
  assert.match(js, /if \(!folders\.length && !canCreateFolder\)/, "no folder and no right to create one: the old warning");
  assert.match(js, /folderIdFromTitle\(title, fill\)/);
  assert.match(js, /folderIdWithSuffix\(id, fill\)/);
  assert.match(js, /testid: "import-new-folder-title"/);
  // A second try after a refusal of the dashboard uses the folder that was made: no second folder.
  assert.match(js, /if \(madeFolder && madeFolder\.title === newTitle\.value\.trim\(\)\) return madeFolder\.id;/);
  // The page tells the dialog that the person can create folders.
  assert.match(read("js/list.js"), /canCreateFolder: editing && folderAccess\.create === true/);
});
