#!/usr/bin/env node
// SPDX-License-Identifier: MIT OR Apache-2.0
// Measures the loading previews of a dashboard against the loaded panels (9.11). It needs a running Hune server and a Chrome (CHROME, default
// /usr/bin/google-chrome). It does not start a server and it does not change the dashboard.
//
// Use:
//   node tests/browser/preview_measure.mjs --url http://127.0.0.1:8080 --uid otel-compute-workloads
//   node tests/browser/preview_measure.mjs --url http://127.0.0.1:8080 --uid latency --ids 1,2 --all --json out.json
//
// Options:
//   --url <origin>    the origin of the server, with the base path if any (required)
//   --uid <uid>       the uid of the dashboard (required)
//   --query <text>    more query text for the page, for example "from=now-1h&to=now" (default: none)
//   --ids <list>      the panels to measure, separated by commas (default: all)
//   --all             print the unclaimed parts and the labels too (default: the parts that the preview claims)
//   --json <file>     write the full result as JSON
//   --width <px>, --height <px>   the viewport (default 1440 by 4000: a tall viewport loads every panel)
//   --profile <dir>   the profile directory of Chrome (default: a folder in the system temporary directory)
//   --coarse          emulate a touch screen: `(pointer: coarse)` is true
//   --strict          exit with code 1 when a row has the verdict fail or no-loaded
// The server must reach its data sources. A panel that fails to load has no loaded measure: its rows show the verdict no-loaded or unclaimed.
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Browser } from "./lib/cdp.mjs";
import { formatTable, measureDashboard } from "./lib/preview_measure.mjs";

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const value = (name, fallback = null) => {
  const i = args.indexOf(name);
  if (i < 0) return fallback;
  if (i + 1 >= args.length || args[i + 1].startsWith("--")) throw new Error(`${name} needs a value`);
  return args[i + 1];
};
const KNOWN = new Set(["--url", "--uid", "--query", "--ids", "--all", "--json", "--width", "--height", "--profile", "--strict", "--coarse"]);
for (const a of args) if (a.startsWith("--") && !KNOWN.has(a)) throw new Error(`unknown option ${a}`);

const url = value("--url");
const uid = value("--uid");
if (!url || !uid) {
  console.error("usage: node tests/browser/preview_measure.mjs --url <origin> --uid <uid> [--ids 1,2] [--all] [--json file] [--strict]");
  process.exit(2);
}
const query = value("--query", "");
const ids = value("--ids") ? value("--ids").split(",") : null;
const page_url = `${url.replace(/\/$/, "")}/d/${encodeURIComponent(uid)}${query ? `?${query}` : ""}`;
const width = Number(value("--width", 1440));
const height = Number(value("--height", 4000));
const profile = value("--profile") || mkdtempSync(join(tmpdir(), "hune-preview-measure-"));

const browser = await Browser.launch({ profileDir: profile, width, height: 1000 });
let result;
try {
  const context = await browser.newContext();
  const page = await context.newPage({ width, height });
  result = await measureDashboard(page, page_url, { ids, width, height, coarse: flag("--coarse") });
} finally {
  await browser.close();
  if (!value("--profile")) rmSync(profile, { recursive: true, force: true });
}

console.log(`dashboard ${uid}: ${Object.keys(result.previews).length} panels`);
for (const id of Object.keys(result.previews)) {
  const p = result.previews[id];
  const l = result.loaded[id];
  const parts = p && p.parts ? p.parts.length : 0;
  console.log(`  panel ${id} ${p ? p.type : "?"}: family ${p && p.family ? p.family : "-"}, preview shown ${p ? p.shown : "-"}, box ${p && p.box ? p.box.join("x") : "-"}, notes ${p && p.notes !== undefined ? p.notes : "-"}, ${parts} preview parts, ${l ? l.parts.length : 0} loaded parts, final state ${l ? l.state : "-"}`);
}
console.log("");
console.log(formatTable(result.rows, { all: flag("--all") }));
console.log("");
console.log(`summary: ${Object.entries(result.summary).map(([k, v]) => `${k} ${v}`).join(", ")}`);
if (value("--json")) writeFileSync(value("--json"), `${JSON.stringify(result, null, 2)}\n`);
if (flag("--strict") && (result.summary.fail > 0 || result.summary["no-loaded"] > 0)) process.exit(1);
