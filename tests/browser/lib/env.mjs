// SPDX-License-Identifier: MIT OR Apache-2.0
// Test environment of the browser suite (16.10): ClickHouse container, volume, configuration, server.
// Every resource that this file starts has a stop function, and the runner calls it on every path.
import { spawn, spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { chmodSync, cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
export const WORK = join(ROOT, "target/browser");
export const DOCKER = process.env.DOCKER || "docker";
export const BINARY = join(ROOT, "target/debug/hune");
export const IMAGE = "clickhouse/clickhouse-server:26.8.34.13";

export const CH_ADMIN_USER = "hune_admin";
export const CH_ADMIN_PASSWORD = "hune-test-admin-password";
export const CH_USER = "hune";
export const CH_PASSWORD = "hune-test-password";

/** Seed range of the fixture table: 30 rows from 1794211200 s, step 20 s. */
export const SEED_FROM = 1794211200000;
export const SEED_TO = 1794212400000;

export function freePort() {
  return new Promise((res, rej) => {
    const s = createServer();
    s.once("error", rej);
    s.listen(0, "127.0.0.1", () => {
      const { port } = s.address();
      s.close(() => res(port));
    });
  });
}

export function sh(cmd, args, opts = {}) {
  return spawnSync(cmd, args, { encoding: "utf8", ...opts });
}

/** Start the ClickHouse container the way `tests/ci/source.sh` does (no TLS). */
export async function startClickHouse() {
  const port = await freePort();
  const name = `hune-test-browser-ch-${process.pid}`;
  sh(DOCKER, ["rm", "-f", "-v", name]);
  const r = sh(DOCKER, [
    "run", "-d", "--name", name,
    "-p", `127.0.0.1:${port}:8123`,
    "-e", `CLICKHOUSE_USER=${CH_ADMIN_USER}`,
    "-e", `CLICKHOUSE_PASSWORD=${CH_ADMIN_PASSWORD}`,
    "-e", "CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT=1",
    "-v", `${ROOT}/tests/fixtures/clickhouse/init.sql:/docker-entrypoint-initdb.d/init.sql:ro`,
    "-v", `${ROOT}/tests/fixtures/clickhouse/config.d/keepalive.xml:/etc/clickhouse-server/config.d/keepalive.xml:ro`,
    IMAGE,
  ]);
  if (r.status !== 0) throw new Error("docker run failed: " + r.stderr);
  const ch = {
    name,
    port,
    url: `http://127.0.0.1:${port}`,
    stop() {
      sh(DOCKER, ["rm", "-f", "-v", name]);
    },
    async query(sql, user = CH_ADMIN_USER, password = CH_ADMIN_PASSWORD) {
      const res = await fetch(ch.url + "/", {
        method: "POST",
        body: sql,
        headers: { Authorization: "Basic " + Buffer.from(`${user}:${password}`).toString("base64") },
      });
      if (!res.ok) throw new Error(`ClickHouse ${res.status}: ${(await res.text()).slice(0, 200)}`);
      return res.text();
    },
  };
  // The init script runs on a first temporary server: wait for the seed table on the final one.
  for (let i = 0; i < 120; i++) {
    try {
      const n = await ch.query("SELECT count() FROM analytics.seed");
      if (n.trim() === "30") return ch;
    } catch {
      // not ready
    }
    await sleep(1000);
  }
  const logs = sh(DOCKER, ["logs", name]).stdout.slice(-1500);
  ch.stop();
  throw new Error("ClickHouse did not become ready\n" + logs);
}

export const PG_IMAGE = "postgres:18";
export const PG_ID = "pg";
/** The window of the synthetic table `metrics.node_load`: 720 minutes from 2026-01-01T00:00:00Z. */
export const PG_FROM = 1767225600000;
export const PG_TO = 1767268800000;

/**
 * Start a disposable PostgreSQL 18 container (the local image `postgres:18`) on 127.0.0.1 and wait for the fixture
 * `tests/fixtures/postgres-source/init.sql`, which `apply.sh` runs at the first start. The passwords are random and
 * live under target/browser/pg/ only. `stop()` removes the container with `docker rm -f -v`.
 * `block` is the configuration block `postgres "pg"` of the role `hune_ro` (8a.13).
 *
 * The container name and the folder come from `tag`. Two callers in one run MUST give two different tags: the call
 * removes a container of the same name first, so a second call with the same tag would remove the container of the
 * first call (B135 did this to B44 and B45). A second call with the same tag throws.
 */
const pgTags = new Set();
export function pgContainerName(tag = "") {
  return `hune-test-browser-pg-${process.pid}${tag ? `-${tag}` : ""}`;
}
export async function startPostgres(tag = "") {
  if (!/^[a-z0-9]*$/.test(tag)) throw new Error(`startPostgres: the tag must match [a-z0-9]*, got ${JSON.stringify(tag)}`);
  if (pgTags.has(tag)) throw new Error(`startPostgres: the tag ${JSON.stringify(tag)} is already in use in this run`);
  pgTags.add(tag);
  const port = await freePort();
  const name = pgContainerName(tag);
  // The container goes away at exit on every path, also when the runner stops in the middle of a case.
  process.on("exit", () => sh(DOCKER, ["rm", "-f", "-v", name]));
  const dir = join(WORK, tag ? `pg-${tag}` : "pg");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const pw = () => randomBytes(16).toString("hex");
  const admin = pw();
  const ro = pw();
  const envFile = join(dir, "env");
  writeFileSync(
    envFile,
    [`POSTGRES_PASSWORD=${admin}`, `PGS_RO_PASSWORD=${ro}`, `PGS_RW_PASSWORD=${pw()}`, `PGS_BAD_PASSWORD=${pw()}`, `PGS_LEGACY_PASSWORD=${pw()}`].join("\n") + "\n",
    { mode: 0o600 },
  );
  chmodSync(envFile, 0o600);
  const passwordFile = join(dir, "hune_ro.password");
  writeFileSync(passwordFile, ro + "\n", { mode: 0o600 });
  chmodSync(passwordFile, 0o600);
  sh(DOCKER, ["rm", "-f", "-v", name]);
  const fixtures = `${ROOT}/tests/fixtures/postgres-source`;
  const r = sh(DOCKER, [
    "run", "-d", "--name", name,
    "-p", `127.0.0.1:${port}:5432`,
    "--env-file", envFile,
    "-v", `${fixtures}:/fixtures:ro`,
    "-v", `${fixtures}/apply.sh:/docker-entrypoint-initdb.d/10-hune-apply.sh:ro`,
    "--tmpfs", "/var/lib/postgresql:size=2g",
    PG_IMAGE,
    "-c", "fsync=off", "-c", "full_page_writes=off",
  ]);
  if (r.status !== 0) throw new Error("docker run (postgres) failed: " + r.stderr);
  const pg = {
    name,
    port,
    passwordFile,
    block: [
      `postgres "${PG_ID}" {`,
      '  label              = "Metrics"',
      '  host               = "127.0.0.1"',
      `  port               = ${port}`,
      '  database           = "analytics"',
      '  user               = "hune_ro"',
      `  password_file      = ${JSON.stringify(passwordFile)}`,
      '  search_path        = "metrics"',
      '  sslmode            = "disable"',
      '  connect_timeout    = "5s"',
      '  max_execution_time = "1m"',
      "  max_result_rows    = 100000",
      "  max_result_bytes   = 33554432",
      "  max_query_size     = 1048576",
      "  max_concurrent     = 16",
      '  queue_wait         = "5s"',
      "}",
    ].join("\n"),
    stop() {
      sh(DOCKER, ["rm", "-f", "-v", name]);
    },
    /** Run one SQL text as the superuser inside the container. */
    query(sql) {
      const q = sh(DOCKER, ["exec", name, "psql", "-h", "127.0.0.1", "-U", "postgres", "-d", "analytics", "-tAc", sql]);
      if (q.status !== 0) throw new Error("psql: " + q.stderr.slice(0, 300));
      return q.stdout.trim();
    },
  };
  // The first temporary server of the entrypoint has no TCP port: wait for the marker table on the final one.
  for (let i = 0; i < 180; i++) {
    const q = sh(DOCKER, ["exec", name, "psql", "-h", "127.0.0.1", "-U", "postgres", "-d", "analytics", "-tAc", "SELECT count(*) FROM metrics.fixture_marker"]);
    if (q.status === 0 && q.stdout.trim() === "1") return pg;
    await sleep(1000);
  }
  const logs = sh(DOCKER, ["logs", name]).stderr.slice(-1500);
  pg.stop();
  throw new Error("PostgreSQL did not become ready\n" + logs);
}

export const MG_ID = "mongo";
export const MG_IMAGE = "mongo:8.0";
/** The window of the synthetic collection `analytics.node_load` of the light fixture: 2000 s from 2026-01-01T00:00:00Z. */
export const MG_FROM = 1767225600000;
export const MG_TO = 1767227600000;

/** The block `mongodb "mongo"` of the role `hune_ro` (8b.13). `caFile` is null with `tls_mode = "disable"`. */
export function mongoBlockText({ port, passwordFile, caFile = null }) {
  return [
    `mongodb "${MG_ID}" {`,
    '  label              = "Events"',
    '  topology           = "direct"',
    '  host               = "localhost"',
    `  port               = ${port}`,
    '  user               = "hune_ro"',
    `  password_file      = ${JSON.stringify(passwordFile)}`,
    '  auth_database      = "admin"',
    '  databases          = ["analytics"]',
    '  collections        = ["node_load", "node_events", "types"]',
    '  read_preference    = "primary"',
    `  tls_mode           = "${caFile ? "verify-full" : "disable"}"`,
    '  connect_timeout    = "5s"',
    '  max_execution_time = "1m"',
    "  max_result_rows    = 100000",
    "  max_result_bytes   = 33554432",
    "  max_query_size     = 1048576",
    "  max_concurrent     = 16",
    '  queue_wait         = "5s"',
    ...(caFile ? ["", "  tls {", `    ca_file = ${JSON.stringify(caFile)}`, "  }"] : []),
    "}",
  ].join("\n");
}

/**
 * Ask the binary whether this build accepts a `mongodb` block. A build with no MongoDB driver refuses the block at
 * stage 2 (8b.2), so `hune check-config` fails with a message that names the driver. No container is needed.
 * Gives `{ driver: true }`, `{ driver: false, message }` (the refusal) or `{ error }` (any other failure).
 */
export function probeMongoDriver() {
  const dir = join(WORK, "mongo-probe");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const passwordFile = join(dir, "password");
  writeFileSync(passwordFile, "probe\n", { mode: 0o600 });
  const volume = join(dir, "volume");
  makeVolume(volume);
  const config = makeConfig({
    listen: "127.0.0.1:1",
    ops: "127.0.0.1:2",
    volume,
    chUrl: "http://127.0.0.1:1",
    passwordFile,
    mongoBlock: mongoBlockText({ port: 27017, passwordFile }),
  });
  const configPath = join(dir, "hune.hcl");
  writeFileSync(configPath, config);
  const r = sh(BINARY, ["check-config", "-c", configPath]);
  rmSync(dir, { recursive: true, force: true });
  if (r.error) return { error: r.error.message };
  if (r.status === 0) return { driver: true };
  const message = ((r.stdout || "") + (r.stderr || "")).trim();
  if (/no MongoDB driver/i.test(message)) return { driver: false, message };
  return { error: "check-config failed for a reason other than the missing driver: " + message.slice(0, 600) };
}

/**
 * Start a disposable MongoDB 8 container on 127.0.0.1 the way `tests/ci/mongosource.sh` does: a CA and a certificate
 * for the name `localhost` (made with `openssl`), TLS only, authorization on, and the light data of
 * `tests/fixtures/mongo-source/init.js` (2000 documents in `node_load`). The entrypoint is `start.sh`. The passwords are
 * random and live under target/browser/mongo/ only. `stop()` removes the container with `docker rm -f -v`.
 * `block` is the configuration block of the role `hune_ro`.
 *
 * The container name and the folder come from `tag`, as for `startPostgres`: two callers in one run MUST give two
 * different tags (B47 and B48 use no tag, B217 uses `an217`).
 */
export async function startMongo(tag = "") {
  if (!/^[a-z0-9]*$/.test(tag)) throw new Error(`startMongo: the tag must match [a-z0-9]*, got ${JSON.stringify(tag)}`);
  const port = await freePort();
  const name = `hune-test-browser-mongo-${process.pid}${tag ? `-${tag}` : ""}`;
  const dir = join(WORK, tag ? `mongo-${tag}` : "mongo");
  rmSync(dir, { recursive: true, force: true });
  const tls = join(dir, "tls");
  mkdirSync(tls, { recursive: true });
  const run = (cmd, args) => {
    const r = sh(cmd, args);
    if (r.status !== 0) throw new Error(`${cmd} ${args[0]} failed: ${(r.stderr || "").slice(0, 300)}`);
  };
  run("openssl", ["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", join(tls, "ca.key"), "-out", join(tls, "ca.pem"), "-days", "2", "-subj", "/CN=hune-test-ca"]);
  run("openssl", ["req", "-newkey", "rsa:2048", "-nodes", "-keyout", join(tls, "server.key"), "-out", join(tls, "server.csr"), "-subj", "/CN=localhost"]);
  writeFileSync(join(tls, "server.ext"), "subjectAltName=DNS:localhost\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n");
  run("openssl", ["x509", "-req", "-in", join(tls, "server.csr"), "-CA", join(tls, "ca.pem"), "-CAkey", join(tls, "ca.key"), "-CAcreateserial", "-out", join(tls, "server.crt"), "-days", "2", "-extfile", join(tls, "server.ext")]);
  writeFileSync(join(tls, "server.pem"), readFileSync(join(tls, "server.crt"), "utf8") + readFileSync(join(tls, "server.key"), "utf8"));
  // The container reads the files as another user. They hold test keys only.
  chmodSync(dir, 0o755);
  chmodSync(tls, 0o755);
  for (const f of ["ca.pem", "server.pem"]) chmodSync(join(tls, f), 0o644);
  const pw = () => randomBytes(16).toString("hex");
  const admin = pw();
  const ro = pw();
  const passwordFile = join(dir, "hune_ro.password");
  writeFileSync(passwordFile, ro + "\n", { mode: 0o600 });
  chmodSync(passwordFile, 0o600);
  sh(DOCKER, ["rm", "-f", "-v", name]);
  const fixtures = `${ROOT}/tests/fixtures/mongo-source`;
  const env = {
    MGS_PORT: String(port),
    MGS_ADMIN_PASSWORD: admin,
    MGS_RO_PASSWORD: ro,
    MGS_RW_PASSWORD: pw(),
    MGS_BAD_PASSWORD: pw(),
    MGS_LEGACY_PASSWORD: pw(),
    MGS_NODE_LOAD_DOCS: "2000",
    MGS_BIG_BYTES: "1048576",
    MGS_CACHE_GB: "0.25",
  };
  const r = sh(DOCKER, [
    "run", "-d", "--name", name,
    "-p", `127.0.0.1:${port}:${port}`,
    ...Object.entries(env).flatMap(([k, v]) => ["-e", `${k}=${v}`]),
    "-v", `${tls}:/tls:ro`,
    "-v", `${fixtures}:/fixtures:ro`,
    "--tmpfs", "/data/db:size=1g",
    "--entrypoint", "/bin/sh",
    MG_IMAGE,
    "/fixtures/start.sh",
  ]);
  if (r.status !== 0) throw new Error("docker run (mongo) failed: " + r.stderr);
  const mg = {
    name,
    port,
    passwordFile,
    caFile: join(tls, "ca.pem"),
    block: mongoBlockText({ port, passwordFile, caFile: join(tls, "ca.pem") }),
    stop() {
      sh(DOCKER, ["rm", "-f", "-v", name]);
    },
  };
  // The temporary server of the first start has no TLS: wait for the marker collection on the final one.
  for (let i = 0; i < 180; i++) {
    const q = sh(DOCKER, [
      "exec", name, "mongosh", "--quiet", "--norc", "--host", "localhost", "--port", String(port), "--tls", "--tlsCAFile", "/tls/ca.pem",
      "--authenticationDatabase", "admin", "-u", "root", "-p", admin,
      "--eval", 'print(db.getSiblingDB("analytics").fixture_marker.countDocuments({}))',
    ]);
    if (q.status === 0 && q.stdout.trim() === "1") return mg;
    await sleep(1000);
  }
  const logs = ((sh(DOCKER, ["logs", name]).stderr || "") + "").slice(-1500);
  mg.stop();
  throw new Error("MongoDB did not become ready\n" + logs);
}

/** The dashboard of the fixture with its SQL on the seed table (as `seed_dashboard()` in real_ch.rs). */
export function seedDashboardText() {
  return readFileSync(join(ROOT, "tests/fixtures/dashboards/nodes.json"), "utf8")
    .replace(/\b[a-z]+\.node_weight\b/g, "analytics.seed")
    .replaceAll("$__timeInterval(ts)", "$__timeInterval(event_time)")
    .replaceAll("$__timeFilter(ts)", "$__timeFilter(event_time)")
    .replaceAll('"SELECT node, name, weight, ratio, status"', '"SELECT node, weight, blocks, active"')
    .replace(/"FROM [a-z]+\.nodes"/g, '"FROM analytics.seed"')
    .replaceAll("OR account = '${account}'", "OR node = '${account}'")
    .replaceAll("AND env = '${env}'", "AND '${env}' = 'prod'");
}

/** Make a volume with the folder `transit` and the dashboard `nodes`. */
export function makeVolume(dir) {
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(join(dir, "transit"), { recursive: true });
  writeFileSync(join(dir, "transit/_folder.json"), JSON.stringify({ format_version: 1, title: "Transit desk" }, null, 2) + "\n");
  writeFileSync(join(dir, "transit/nodes.json"), seedDashboardText());
  // A second dashboard, valid, with no tag: the list has two cards (the filters and the sort show with the threshold 0). A volume has no file that is not
  // valid at the start (5.41): the cases of a file that is not valid (B01) break this file after the start, and the page shows a stale card.
  const spare = JSON.parse(seedDashboardText());
  spare.uid = "stale-one";
  spare.title = "Stale one";
  spare.tags = [];
  writeFileSync(join(dir, "transit/stale-one.json"), JSON.stringify(spare, null, 2) + "\n");
}

/** Make a read-only volume `lab` with two folders: `lab-ops` (the dashboard `lab-traffic`) and `lab-archive` (the dashboard `lab-inventory`). */
export function makeLabVolume(dir) {
  rmSync(dir, { recursive: true, force: true });
  for (const [folder, title, uid, docTitle] of [["lab-ops", "Lab operations", "lab-traffic", "Lab traffic"], ["lab-archive", "Lab archive", "lab-inventory", "Lab inventory"]]) {
    mkdirSync(join(dir, folder), { recursive: true });
    writeFileSync(join(dir, folder, "_folder.json"), JSON.stringify({ format_version: 1, title }, null, 2) + "\n");
    const doc = JSON.parse(seedDashboardText());
    doc.uid = uid;
    doc.title = docTitle;
    doc.tags = [];
    writeFileSync(join(dir, folder, `${uid}.json`), JSON.stringify(doc, null, 2) + "\n");
  }
}

/** Add a dashboard with no folder to a volume: the file `<uid>.json` at the root of the volume path (5.9). The seed volumes have none, so the cases that
 *  count cards keep their numbers; a case that needs root dashboards asks for them with the options `prepare` (the primary) and `labPrepare` (the store lab). */
export function addRootDashboard(dir, uid, title, tags = []) {
  const doc = JSON.parse(seedDashboardText());
  doc.uid = uid;
  doc.title = title;
  doc.tags = tags;
  writeFileSync(join(dir, `${uid}.json`), JSON.stringify(doc, null, 2) + "\n");
}

/** Build the configuration text from `print-reference` with the values of this run. */
export function makeConfig({ listen, ops, volume, labVolume = null, chUrl, passwordFile, editing = true, scratchVolume = null, readOnlyStore = false, history = true, export: exportKey = true, basePath = "/", accessBlock = null, pgBlock = null, mongoBlock = null, queryInspect = null, serverLines = [], geomapBlock = null, chLines = [] }) {
  const ref = sh(BINARY, ["print-reference", "--format", "hcl", "--profile", "local"]);
  if (ref.status !== 0) throw new Error("print-reference failed: " + ref.stderr);
  let t = ref.stdout;
  const set = (key, value, once = true) => {
    const re = new RegExp(`^(\\s*${key}\\s*=\\s*).*$`, "m");
    if (!re.test(t)) throw new Error("key not in the reference: " + key);
    t = t.replace(re, (_, p) => p + value);
    return once;
  };
  set("listen", JSON.stringify(listen));
  set("ops_listen", JSON.stringify(ops));
  set("base_path", JSON.stringify(basePath));
  // The keys `server.history` and `server.export` are mandatory (13.8): the reference has them with `true`.
  set("history", String(history));
  set("export", String(exportKey));
  // A file needs exactly one primary store (13.10). `editing = false` makes the seed volume a read-only store, with a title, and adds a second,
  // empty volume `scratch` as the primary store. The cases of this mode look at the dashboards of the seed volume, which have no edit and no history.
  if (!editing) {
    if (!scratchVolume) throw new Error("editing = false needs the path of the scratch volume");
    if (!/^\s*primary\s*=\s*true\n/m.test(t)) throw new Error("no primary store in the reference");
    t = t.replace(/^(\s*)primary\s*=\s*true\n/m, (m, a) => `${a}title           = "Transit"\n`);
    t = t.replace(/^\s*history_keep\s*=\s*\d+\n/m, "");
    const re = /^(storage \{[\s\S]*?)^\}/m;
    t = t.replace(re, (m, body) => `${body}\n  volume "scratch" {\n    path            = ${JSON.stringify(scratchVolume)}\n    primary         = true\n    rescan_interval = "1s"\n    history_keep    = 100\n  }\n}`);
  }
  // `readOnlyStore` makes a file with no primary store: the one volume is read-only, `storage.read_only = true` says it, and nothing is recorded
  // (`server.history = false`, C35), so the volume has no `history_keep` (C31).
  if (readOnlyStore) {
    if (!editing) throw new Error("readOnlyStore and editing = false exclude each other");
    t = t.replace(/^(\s*)primary\s*=\s*true\n/m, (m, a) => `${a}title           = "Transit"\n`);
    t = t.replace(/^\s*history_keep\s*=\s*\d+\n/m, "");
    t = t.replace(/^storage \{\n/m, "storage {\n  read_only = true\n\n");
    t = t.replace(/^(\s*history\s*=\s*).*$/m, "$1false");
  }
  // `server.query_inspect` has no line in the reference (its default is `false`): the line is added after `base_path` (B46).
  if (queryInspect !== null) {
    if (/^\s*query_inspect\s*=/m.test(t)) throw new Error("query_inspect is already in the reference");
    t = t.replace(/^(\s*)(base_path\s*=.*)$/m, (m, a) => `${m}\n${a}query_inspect = ${queryInspect ? "true" : "false"}`);
  }
  // More keys of the block `server` (B101: `repeat_max_per_*`): each line is added after `base_path`. A key that the reference has is an error.
  for (const line of serverLines) {
    const key = line.split("=")[0].trim();
    if (new RegExp(`^\\s*${key}\\s*=`, "m").test(t)) throw new Error(`${key} is already in the reference`);
    t = t.replace(/^(\s*)(base_path\s*=.*)$/m, (m, a) => `${m}\n${a}${line}`);
  }
  set("path", JSON.stringify(volume));
  set("rescan_interval", JSON.stringify("1s"));
  // A read-only volume (B241): a block `volume "lab"` with a title is added at the end of the block `storage`.
  if (labVolume) {
    const re = /^(storage \{[\s\S]*?)^\}/m;
    if (!re.test(t)) throw new Error("no storage block in the reference");
    t = t.replace(re, (m, body) => `${body}\n  volume "lab" {\n    title           = "Lab"\n    path            = ${JSON.stringify(labVolume)}\n    rescan_interval = "1s"\n  }\n}`);
  }
  set("url", JSON.stringify(chUrl));
  set("user", JSON.stringify(CH_USER));
  set("password_file", JSON.stringify(passwordFile));
  // Another access mode (B23): the block `access { ... }` of the reference is replaced.
  if (accessBlock) {
    const re = /^access \{[\s\S]*?^\}\n/m;
    if (!re.test(t)) throw new Error("no access block in the reference");
    t = t.replace(re, () => accessBlock.trimEnd() + "\n");
  }
  // Remove the `tls` block of the source.
  t = t.replace(/\n[ \t]*tls \{[^}]*\}\n/, "\n");
  if (/\btls\b/.test(t.replace(/#.*$/gm, ""))) throw new Error("the tls block is still in the configuration");
  // A second source that is always down (nothing listens on port 1): B24.
  const block = t.match(/clickhouse "prod" \{[\s\S]*\n\}\n?/);
  if (!block) throw new Error("no source block in the reference");
  const down = block[0]
    .replace('clickhouse "prod"', 'clickhouse "zdown"')
    .replace(/label\s*=\s*".*"/, 'label              = "Down"')
    .replace(/url\s*=\s*".*"/, 'url                = "http://127.0.0.1:1"')
    .replace(/connect_timeout\s*=\s*".*"/, 'connect_timeout    = "1s"');
  t = t.trimEnd() + "\n\n" + down;
  // More keys of the block `clickhouse "prod"` only (B218: `adhoc_columns`): each line goes after the first line of the block.
  for (const line of chLines) t = t.replace(/^clickhouse "prod" \{\n/m, (m) => `${m}  ${line}\n`);
  // The mode authzen needs the limit for each subject on each source (11.22).
  if (accessBlock) t = t.replace(/^(\s*)queue_wait(\s*)=\s*(.*)$/gm, (m, a, b, v) => `${m}\n${a}max_concurrent_per_subject = 6`);
  // A PostgreSQL source (B44, B45): the block is added at the end, after the ClickHouse sources.
  if (pgBlock) t = t.trimEnd() + "\n\n" + pgBlock.trimEnd() + "\n";
  // A MongoDB source (B47, B48): the block is added after the other sources.
  if (mongoBlock) t = t.trimEnd() + "\n\n" + mongoBlock.trimEnd() + "\n";
  // A block `geomap` (B227, 13.12c): it goes at the end of the file.
  if (geomapBlock) t = t.trimEnd() + "\n\n" + geomapBlock.trimEnd() + "\n";
  // With `server.history = false` the primary store keeps no earlier version and has no `history_keep` (C31).
  if (!history) t = t.replace(/^\s*history_keep\s*=\s*\d+\n/gm, "");
  return t;
}

/**
 * Start `hune serve` with a volume and a configuration.
 * `name` names the files under target/browser/<name>/.
 */
export async function startServer(name, ch, opts = {}) {
  const dir = join(WORK, name);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const volume = join(dir, "volume");
  makeVolume(volume);
  if (opts.prepare) opts.prepare(volume);
  const scratchVolume = (opts.editing ?? true) ? null : join(dir, "scratch");
  if (scratchVolume) mkdirSync(scratchVolume, { recursive: true });
  const labVolume = opts.lab ? join(dir, "lab") : null;
  if (labVolume) {
    makeLabVolume(labVolume);
    if (opts.labPrepare) opts.labPrepare(labVolume);
  }
  const passwordFile = join(dir, "ch-password");
  writeFileSync(passwordFile, CH_PASSWORD + "\n");
  const port = await freePort();
  const opsPort = await freePort();
  const config = makeConfig({
    listen: `127.0.0.1:${port}`,
    ops: `127.0.0.1:${opsPort}`,
    volume,
    labVolume,
    chUrl: ch.url,
    passwordFile,
    editing: opts.editing ?? true,
    scratchVolume,
    readOnlyStore: opts.readOnlyStore ?? false,
    history: opts.readOnlyStore ? false : opts.history ?? true,
    export: opts.export ?? true,
    basePath: opts.basePath ?? "/",
    accessBlock: opts.accessBlock ?? null,
    pgBlock: opts.postgres ? opts.postgres.block : null,
    mongoBlock: opts.mongodb ? opts.mongodb.block : null,
    geomapBlock: opts.geomapBlock ?? null,
    queryInspect: opts.queryInspect ?? null,
    serverLines: opts.serverLines || [],
    chLines: opts.chLines || [],
  });
  const configPath = join(dir, "hune.hcl");
  writeFileSync(configPath, config);
  const check = sh(BINARY, ["check-config", "-c", configPath]);
  if (check.status !== 0) throw new Error("check-config failed: " + check.stdout + check.stderr);
  const logPath = join(dir, "server.log");
  const { openSync } = await import("node:fs");
  let proc = null;
  let exited = null;
  const launch = () => {
    const fd = openSync(logPath, "a");
    exited = null;
    const child = spawn(BINARY, ["serve", "-c", configPath], { stdio: ["ignore", fd, fd], env: {}, detached: true });
    child.on("exit", (code, sig) => {
      if (child === proc) exited = { code, sig };
    });
    proc = child;
    server.proc = child;
  };
  const waitReady = async () => {
    for (let i = 0; i < 150; i++) {
      if (exited) throw new Error(`server exited at start: ${JSON.stringify(exited)}\n${server.log().slice(-1500)}`);
      try {
        const r = await fetch(server.opsOrigin + "/readyz");
        if (r.status === 200) return;
      } catch {
        // not listening yet
      }
      await sleep(100);
    }
    server.killNow();
    throw new Error("/readyz did not answer 200\n" + server.log().slice(-1500));
  };
  const server = {
    name,
    dir,
    volume,
    logPath,
    configPath,
    origin: `http://127.0.0.1:${port}`,
    basePath: opts.basePath ?? "/",
    opsOrigin: `http://127.0.0.1:${opsPort}`,
    proc: null,
    get url() {
      return server.origin + server.basePath;
    },
    async stop() {
      if (!proc) return null;
      if (exited) return exited;
      proc.kill("SIGTERM");
      for (let i = 0; i < 100 && !exited; i++) await sleep(100);
      if (!exited) {
        proc.kill("SIGKILL");
        for (let i = 0; i < 20 && !exited; i++) await sleep(100);
      }
      return exited;
    },
    /** Start again on the same volume and configuration (after stop). */
    async restart() {
      launch();
      await waitReady();
    },
    killNow() {
      try {
        process.kill(-proc.pid, "SIGKILL");
      } catch {
        // ignore
      }
    },
    log() {
      return readFileSync(logPath, "utf8");
    },
  };
  launch();
  await waitReady();
  return server;
}

/** The JSON API of a server for the setup and the checks of the cases. */
export function api(server) {
  const base = server.url;
  const uuid = () => crypto.randomUUID();
  async function call(method, path, { body, headers = {}, raw = false } = {}) {
    const h = { ...headers };
    let payload;
    if (body !== undefined) {
      h["Content-Type"] = "application/json";
      payload = typeof body === "string" ? body : JSON.stringify(body);
    }
    const res = await fetch(base + path, { method, headers: h, body: payload });
    const text = await res.text();
    let json = null;
    if (!raw) {
      try {
        json = JSON.parse(text);
      } catch {
        json = null;
      }
    }
    return { status: res.status, headers: res.headers, text, json, etag: (res.headers.get("etag") || "").replaceAll('"', "") };
  }
  return {
    call,
    uuid,
    get: (path) => call("GET", path),
    session: () => call("GET", "api/session"),
    dashboard: (uid) => call("GET", `api/dashboards/${uid}`),
    download: (uid) => call("GET", `api/dashboards/${uid}?download=true`, { raw: true }),
    history: (uid) => call("GET", `api/dashboards/${uid}/history`),
    list: () => call("GET", "api/dashboards?limit=500"),
    create: (folder, document, message = "") =>
      call("POST", "api/dashboards", { body: { folder, message, document }, headers: { "Idempotency-Key": uuid() } }),
    save: (uid, etag, document, message = "") =>
      call("PUT", `api/dashboards/${uid}`, { body: { message, document }, headers: { "If-Match": `"${etag}"`, "Idempotency-Key": uuid() } }),
    del: (uid, etag) => call("DELETE", `api/dashboards/${uid}`, { headers: { "If-Match": `"${etag}"`, "Idempotency-Key": uuid() } }),
    createFolder: (id, title) => call("POST", "api/folders", { body: { id, title } }),
  };
}

/** A copy of the seed dashboard under another uid and title, as an object. */
export function seedDocument(uid, title = null) {
  const d = JSON.parse(seedDashboardText());
  d.uid = uid;
  if (title) d.title = title;
  return d;
}

export { cpSync };
