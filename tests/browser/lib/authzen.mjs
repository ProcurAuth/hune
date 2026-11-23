// SPDX-License-Identifier: MIT OR Apache-2.0
// Mode `authzen` for the browser case B23 (16.10): `fake-pdp`, keys, identity tokens and a policy.
// The ProcurAuth tools `fake-pdp` and `access-token` run from a build that the suite never changes.
// Set PROCURAUTH_DIR to the checkout (default: target/procurauth). The tools are in target/release/.
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, openSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { ROOT, WORK, freePort, seedDocument, startServer } from "./env.mjs";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export const SERVICE = "hune-browser";
export const HEADER = "X-Identity-Token";
const ISSUER = "https://edge.example";
const PDP_CREDENTIAL = "browser-pdp-credential-test";

/** The paths of the two ProcurAuth tools, or null when one is missing. */
export function procurauthTools() {
  const dir = process.env.PROCURAUTH_DIR || join(ROOT, "target/procurauth");
  const fakePdp = join(dir, "target/release/fake-pdp");
  const accessToken = join(dir, "target/release/access-token");
  return existsSync(fakePdp) && existsSync(accessToken) ? { fakePdp, accessToken } : null;
}

/** The grants of one subject: `{ action: [resource paths below the service] }`. */
function grants(spec) {
  const out = [];
  for (const [action, paths] of Object.entries(spec)) {
    for (const p of paths) out.push({ action, resource: p ? `service:${SERVICE}/${p}` : `service:${SERVICE}` });
  }
  return out;
}

/**
 * The policy of the case. Folders: transit (the seed), desk, secret, lab.
 * - admin: all three actions on the service.
 * - reader: read on the folder transit, and on the dashboard desk-open only.
 * - author: read and write on transit and lab. No `source.query`.
 * - sqlauthor: like author, with `source.query` on the source prod.
 * - mover: like sqlauthor, but with `dashboard.write` on transit only.
 * - nobody: a known subject with no grant.
 * - solo: read on the dashboard desk-open only: one visible dashboard (B35).
 */
export function policy(actionDocument) {
  return {
    services: [{ id: SERVICE, document: actionDocument }],
    subjects: [
      { id: "admin", grants: grants({ "dashboard.read": [""], "dashboard.write": [""], "source.query": [""] }) },
      { id: "reader", grants: grants({ "dashboard.read": ["folder:transit", "folder:desk/dashboard:desk-open"] }) },
      { id: "author", grants: grants({ "dashboard.read": ["folder:transit", "folder:lab"], "dashboard.write": ["folder:transit", "folder:lab"] }) },
      {
        id: "sqlauthor",
        grants: grants({ "dashboard.read": ["folder:transit", "folder:lab"], "dashboard.write": ["folder:transit", "folder:lab"], "source.query": ["source:prod"] }),
      },
      {
        id: "mover",
        grants: grants({ "dashboard.read": ["folder:transit", "folder:lab"], "dashboard.write": ["folder:transit"], "source.query": ["source:prod"] }),
      },
      { id: "nobody", grants: [] },
      { id: "solo", grants: grants({ "dashboard.read": ["folder:desk/dashboard:desk-open"] }) },
    ],
  };
}

/** The volume of the case: the folders desk, secret and lab beside transit. */
function prepareVolume(volume) {
  const put = (folder, title, docs) => {
    mkdirSync(join(volume, folder), { recursive: true });
    writeFileSync(join(volume, folder, "_folder.json"), JSON.stringify({ format_version: 1, title }, null, 2) + "\n");
    for (const [uid, docTitle] of docs) {
      writeFileSync(join(volume, folder, `${uid}.json`), JSON.stringify(seedDocument(uid, docTitle), null, 2) + "\n");
    }
  };
  put("desk", "Desk", [["desk-open", "Desk open"], ["desk-closed", "Desk closed"]]);
  put("secret", "Secret", [["hidden-one", "Hidden one"]]);
  put("lab", "Lab", [["lab-a", "Lab A"]]);
}

function sh(cmd, args, opts = {}) {
  return spawnSync(cmd, args, { encoding: "utf8", ...opts });
}

/**
 * Start `hune serve` in mode `authzen`, then `fake-pdp` with the policy.
 * Returns `{ server, pdp, token(sub, opts), load(policy), failures(list), stopPdp(), startPdp() }`.
 * The caller registers `stop` and `killNow` of `server` and `pdp`.
 */
export async function startAuthzen(ch, name, tools, { cacheTtl = "0s", pdpTimeout = "1s" } = {}) {
  const dir = join(WORK, name);
  mkdirSync(dir, { recursive: true });
  // Keys of the test edge: one Ed25519 pair made for this run.
  const priv = join(dir, "edge-ed25519.pem");
  const pub = join(dir, "edge-ed25519.pub.pem");
  let r = sh("openssl", ["genpkey", "-algorithm", "ed25519", "-out", priv]);
  if (r.status !== 0) throw new Error("openssl genpkey failed: " + r.stderr);
  r = sh("openssl", ["pkey", "-in", priv, "-pubout", "-out", pub]);
  if (r.status !== 0) throw new Error("openssl pkey failed: " + r.stderr);
  const credFile = join(dir, "pdp-credential.txt");
  writeFileSync(credFile, PDP_CREDENTIAL + "\n");
  const pdpPort = await freePort();

  const accessBlock = `access {
  mode = "authzen"

  identity {
    header           = "${HEADER}"
    issuer           = "${ISSUER}"
    audience         = "${SERVICE}"
    public_keys_file = "${pub}"
    max_age          = "5m"
    clock_skew       = "30s"
  }

  pdp {
    url             = "http://127.0.0.1:${pdpPort}"
    token_file      = "${credFile}"
    plain_http      = true
    timeout         = "${pdpTimeout}"
    cache_ttl       = "${cacheTtl}"
    max_connections = 4
  }
}
`;
  const server = await startServer(name + "-hune", ch, { accessBlock, prepare: prepareVolume });

  // The action document of Hune is the document of the service in the policy.
  const res = await fetch(server.origin + "/.well-known/procurauth/actions");
  if (res.status !== 200) throw new Error(`action document: ${res.status}`);
  const actionDocument = await res.json();
  const dataFile = join(dir, "policy.json");
  writeFileSync(dataFile, JSON.stringify(policy(actionDocument), null, 2));

  const pdpUrl = `http://127.0.0.1:${pdpPort}`;
  let pdpProc = null;
  let pdpExit = null;
  const pdp = {
    url: pdpUrl,
    log: () => (existsSync(join(dir, "fake-pdp.log")) ? readFileSync(join(dir, "fake-pdp.log"), "utf8") : ""),
    async start() {
      const fd = openSync(join(dir, "fake-pdp.log"), "a");
      pdpExit = null;
      const child = spawn(tools.fakePdp, ["--listen", `127.0.0.1:${pdpPort}`, "--admin", "--credential-file", credFile, dataFile], {
        stdio: ["ignore", fd, fd],
        env: {},
        detached: true,
      });
      child.on("exit", (code, sig) => {
        if (child === pdpProc) pdpExit = { code, sig };
      });
      pdpProc = child;
      for (let i = 0; i < 100; i++) {
        if (pdpExit) throw new Error("fake-pdp exited at start: " + JSON.stringify(pdpExit) + "\n" + pdp.log().slice(-1500));
        try {
          const m = await fetch(pdpUrl + "/.well-known/authzen-configuration");
          if (m.status === 200) return;
        } catch {
          // not listening yet
        }
        await sleep(50);
      }
      throw new Error("fake-pdp did not answer\n" + pdp.log().slice(-1500));
    },
    async stop() {
      if (!pdpProc || pdpExit) return pdpExit;
      pdpProc.kill("SIGTERM");
      for (let i = 0; i < 100 && !pdpExit; i++) await sleep(50);
      if (!pdpExit) pdpProc.kill("SIGKILL");
      for (let i = 0; i < 50 && !pdpExit; i++) await sleep(50);
      return pdpExit;
    },
    killNow() {
      try {
        process.kill(-pdpProc.pid, "SIGKILL");
      } catch {
        // gone
      }
    },
    /** An admin call of the PDP, with the credential of the PEP. */
    async admin(method, path, body) {
      const q = await fetch(pdpUrl + path, {
        method,
        headers: { Authorization: "Bearer " + PDP_CREDENTIAL, "Content-Type": "application/json" },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      const text = await q.text();
      if (!q.ok) throw new Error(`fake-pdp ${method} ${path}: ${q.status} ${text.slice(0, 300)}`);
      return text ? JSON.parse(text) : null;
    },
  };
  await pdp.start();

  /** An identity token of the test edge. `ageSeconds` > 0 makes an old token: it ended `ageSeconds - 300` s ago. */
  function token(sub, { ageSeconds = 0, lifetime = 300, aud = SERVICE } = {}) {
    const now = Math.floor(Date.now() / 1000) - ageSeconds;
    const claims = join(dir, `claims-${sub}-${ageSeconds}.json`);
    writeFileSync(claims, JSON.stringify({ iss: ISSUER, sub, aud, iat: now, exp: now + lifetime }));
    const out = sh(tools.accessToken, ["make", "--key", priv, claims]);
    if (out.status !== 0) throw new Error("access-token make failed: " + out.stdout + out.stderr);
    return out.stdout.trim();
  }

  return {
    server,
    pdp,
    token,
    /** The header object for `Network.setExtraHTTPHeaders`. */
    headers: (sub, opts) => ({ [HEADER]: token(sub, opts) }),
    /** The JSON API of the server with the token of a subject. */
    call(sub, method, path, { body, headers = {} } = {}) {
      return fetch(server.url + path, {
        method,
        headers: { [HEADER]: token(sub), ...(body !== undefined ? { "Content-Type": "application/json" } : {}), ...headers },
        body: body === undefined ? undefined : typeof body === "string" ? body : JSON.stringify(body),
      }).then(async (x) => {
        const text = await x.text();
        let json = null;
        try {
          json = JSON.parse(text);
        } catch {
          json = null;
        }
        return { status: x.status, text, json, etag: (x.headers.get("etag") || "").replaceAll('"', "") };
      });
    },
    async stop() {
      await pdp.stop();
      return server.stop();
    },
    killNow() {
      pdp.killNow();
      server.killNow();
    },
  };
}
