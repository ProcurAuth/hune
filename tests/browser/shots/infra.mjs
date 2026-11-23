// Infrastructure of the screenshot harness: the volume copy, the configuration and the server.
// The harness uses the local ClickHouse container (it never starts or stops it) and the debug binary.
import { spawn } from "node:child_process";
import { cpSync, existsSync, mkdirSync, openSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { BINARY, ROOT, freePort, makeConfig, sh } from "../lib/env.mjs";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export const LOCAL = join(ROOT, "target/local");
export const SHOTS = join(ROOT, "target/shots");
export const CH_URL = "http://127.0.0.1:18123";
export const CH_PASSWORD_FILE = join(LOCAL, "ch-password");

/** Time range of the demo data, so that every screenshot shows the same chart: the last 24 h of the data. */
export const DATA_TO = Date.parse("2026-11-10T16:34:00Z");
export const DATA_FROM = DATA_TO - 24 * 3600 * 1000;

/** Copy the local volume without `.hune/` (the lock and the internal state). */
export function copyVolume(dest) {
  rmSync(dest, { recursive: true, force: true });
  mkdirSync(dest, { recursive: true });
  const src = join(LOCAL, "volume");
  for (const name of readdirSync(src)) {
    if (name === ".hune" || name.startsWith(".")) continue;
    cpSync(join(src, name), join(dest, name), { recursive: true });
  }
  // A leftover temporary name of the generator is not a document.
  for (const dir of readdirSync(dest, { withFileTypes: true })) {
    if (!dir.isDirectory()) continue;
    for (const f of readdirSync(join(dest, dir.name))) if (f.includes(".tmp-")) rmSync(join(dest, dir.name, f), { force: true });
  }
}

/** Start `hune serve` on free local ports with the volume. Returns { origin, url, stop(), killNow(), log() }. */
export async function startServer({ name, dir, volume, editing = true }) {
  if (!existsSync(BINARY)) throw new Error(`${BINARY} does not exist: build it with cargo build -p hune --locked`);
  if (!existsSync(CH_PASSWORD_FILE)) throw new Error(`${CH_PASSWORD_FILE} does not exist`);
  mkdirSync(dir, { recursive: true });
  const port = await freePort();
  const opsPort = await freePort();
  const config = makeConfig({
    listen: `127.0.0.1:${port}`,
    ops: `127.0.0.1:${opsPort}`,
    volume,
    chUrl: CH_URL,
    passwordFile: CH_PASSWORD_FILE,
    editing,
  });
  const configPath = join(dir, `${name}.hcl`);
  writeFileSync(configPath, config);
  const check = sh(BINARY, ["check-config", "-c", configPath]);
  if (check.status !== 0) throw new Error("check-config failed: " + check.stdout + check.stderr);
  const logPath = join(dir, `${name}.log`);
  const fd = openSync(logPath, "a");
  const child = spawn(BINARY, ["serve", "-c", configPath], { stdio: ["ignore", fd, fd], env: {}, detached: true });
  let exited = null;
  child.on("exit", (code, sig) => {
    exited = { code, sig };
  });
  const server = {
    name,
    volume,
    origin: `http://127.0.0.1:${port}`,
    url: `http://127.0.0.1:${port}/`,
    opsOrigin: `http://127.0.0.1:${opsPort}`,
    log: () => readFileSync(logPath, "utf8"),
    killNow() {
      try {
        process.kill(-child.pid, "SIGKILL");
      } catch {
        // already gone
      }
    },
    async stop() {
      if (exited) return exited;
      child.kill("SIGTERM");
      for (let i = 0; i < 100 && !exited; i++) await sleep(100);
      if (!exited) {
        server.killNow();
        for (let i = 0; i < 20 && !exited; i++) await sleep(100);
      }
      return exited;
    },
  };
  for (let i = 0; i < 150; i++) {
    if (exited) throw new Error(`server ${name} exited at start: ${JSON.stringify(exited)}\n${server.log().slice(-1500)}`);
    try {
      const r = await fetch(server.opsOrigin + "/readyz");
      if (r.status === 200) return server;
    } catch {
      // not listening yet
    }
    await sleep(100);
  }
  server.killNow();
  throw new Error(`server ${name}: /readyz did not answer 200\n${server.log().slice(-1500)}`);
}

/** A small JSON client of one server (editing mode, mode open: no token). */
export function client(server) {
  const call = async (method, path, { body, headers = {} } = {}) => {
    const h = { ...headers };
    if (body !== undefined) h["Content-Type"] = "application/json";
    const res = await fetch(server.url + path, { method, headers: h, body: body === undefined ? undefined : JSON.stringify(body) });
    const text = await res.text();
    let json = null;
    try {
      json = JSON.parse(text);
    } catch {
      // not JSON
    }
    return { status: res.status, json, text, etag: (res.headers.get("etag") || "").replaceAll('"', "") };
  };
  const idem = () => ({ "Idempotency-Key": crypto.randomUUID() });
  return {
    call,
    get: (path) => call("GET", path),
    dashboard: (uid) => call("GET", `api/dashboards/${uid}`),
    create: (folder, document, message = "") => call("POST", "api/dashboards", { body: { folder, message, document }, headers: idem() }),
    save: (uid, etag, document, message = "") =>
      call("PUT", `api/dashboards/${uid}`, { body: { message, document }, headers: { ...idem(), "If-Match": `"${etag}"` } }),
  };
}
