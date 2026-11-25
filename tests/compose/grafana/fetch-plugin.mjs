// SPDX-License-Identifier: MIT OR Apache-2.0
// Download the ClickHouse plugin archive of Grafana, check its SHA-256 and unpack it (16.13).
// Input: the variable PLUGIN_REF, the line of tests/images.lock: `<url>@sha256:<64 hex digits>`.
// Argument: the directory for the plugins. The tool fails when the digest is absent or differs.
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join } from "node:path";

const ref = process.env.PLUGIN_REF ?? "";
const dir = process.argv[2];
const m = /^(https:\/\/[^\s@]+)@sha256:([0-9a-f]{64})$/.exec(ref);
if (!m || !dir) {
  console.error(`fetch-plugin: PLUGIN_REF needs the form <https URL>@sha256:<digest>; pin the archive in tests/images.lock`);
  process.exit(2);
}
// The download goes over the network. A connect timeout or a 5xx answer is a fault of the network, not of the archive,
// so the tool tries up to 5 times. The check of the digest below stays the same for each try.
let res;
for (let attempt = 1; attempt <= 5; attempt++) {
  try {
    res = await fetch(m[1]);
    if (res.status < 500) break;
    console.error(`fetch-plugin: HTTP ${res.status} (try ${attempt} of 5)`);
  } catch (e) {
    console.error(`fetch-plugin: ${e.message}: ${e.cause?.code ?? "no code"} (try ${attempt} of 5)`);
    if (attempt === 5) throw e;
  }
  await new Promise((resolve) => setTimeout(resolve, 3000 * attempt));
}
if (!res.ok) {
  console.error(`fetch-plugin: HTTP ${res.status} for the archive`);
  process.exit(1);
}
const bytes = Buffer.from(await res.arrayBuffer());
const got = createHash("sha256").update(bytes).digest("hex");
if (got !== m[2]) {
  console.error(`fetch-plugin: the archive has SHA-256 ${got}; tests/images.lock says ${m[2]}`);
  process.exit(1);
}
mkdirSync(dir, { recursive: true });
const zip = join(dir, "plugin.zip");
writeFileSync(zip, bytes);
const r = spawnSync("unzip", ["-q", "-o", zip, "-d", dir], { stdio: "inherit" });
if (r.status !== 0) process.exit(1);
spawnSync("rm", ["-f", zip]);
console.log("fetch-plugin: ok");
