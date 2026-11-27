// SPDX-License-Identifier: MIT OR Apache-2.0
// The shell preloads the static closure of main.js (9.6), so the browser does not find the modules level by level.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join, normalize, relative } from "node:path";
import { fileURLToPath } from "node:url";

const WEB = join(dirname(fileURLToPath(import.meta.url)), "../../web");
const JS = join(WEB, "js");

// The files that a module imports with a static `import ... from`, `export ... from` or `import "..."`. A dynamic `import()` is not a static import.
export function staticImports(file) {
  const src = readFileSync(file, "utf8");
  const out = [];
  for (const m of src.matchAll(/\bfrom\s*["'](\.[^"']+)["']|^import\s*["'](\.[^"']+)["']/gm)) out.push(normalize(join(dirname(file), m[1] || m[2])));
  return out;
}

// The static closure of a module in the order of discovery: breadth first, the imports of a module in source order. The entry comes first.
export function staticClosure(entry) {
  const seen = [join(JS, entry)];
  for (let i = 0; i < seen.length; i++) for (const dep of staticImports(seen[i])) if (!seen.includes(dep)) seen.push(dep);
  return seen.map((f) => relative(JS, f));
}

const shell = readFileSync(join(WEB, "shell.html"), "utf8");

test("the shell preloads exactly the static closure of main.js, in the order of discovery", () => {
  const closure = staticClosure("main.js");
  assert.equal(closure[0], "main.js");
  const links = [...shell.matchAll(/<link rel="modulepreload" href="static\/\{\{build\}\}\/js\/([^"]+)">/g)].map((m) => m[1]);
  assert.deepEqual(links, closure.slice(1));
  // The count of the tags is the count of the matches: no other form of modulepreload hides in the shell.
  assert.equal([...shell.matchAll(/rel="modulepreload"/g)].length, links.length);
});

test("the preloads come after the module script of main.js, so the request for main.js is not queued behind them", () => {
  const first = shell.indexOf('rel="modulepreload"');
  const mainScript = shell.indexOf('<script type="module" src="static/{{build}}/js/main.js"></script>');
  assert.ok(mainScript > shell.lastIndexOf('rel="stylesheet"'), "main.js comes after the style sheets");
  assert.ok(shell.indexOf("js/boot_theme.js") < mainScript, "boot_theme.js comes before main.js");
  assert.ok(first > mainScript, "the preloads follow the script of main.js");
  // The marker of the route links follows the links of main.js, and nothing comes after it but the end of the head.
  assert.ok(shell.indexOf("{{preload}}") > shell.lastIndexOf('rel="modulepreload"'));
  assert.ok(shell.indexOf("</head>") > shell.indexOf("{{preload}}"));
});

test("the shell preloads the sprite as a fetch with the same mode as main.js (credentials same-origin), once", () => {
  const links = [...shell.matchAll(/<link rel="preload"[^>]*icons\.svg[^>]*>/g)].map((m) => m[0]);
  assert.deepEqual(links, ['<link rel="preload" href="static/{{build}}/icons.svg" as="fetch" crossorigin>']);
  // `crossorigin` without a value is the mode "anonymous": credentials "same-origin". The fetch of main.js must use the same mode and the same URL.
  const main = readFileSync(join(JS, "main.js"), "utf8");
  assert.match(main, /fetch\(assetUrl\("icons\.svg"\), \{ credentials: "same-origin" \}\)/);
  assert.equal(readFileSync(join(JS, "config.js"), "utf8").match(/return `static\/\$\{boot\.build\}\/\$\{path\}`/)?.length, 1);
});
