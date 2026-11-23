// Self check of the fixture of the job `mongosource` (16.9b): does init.js give the users and the data that the
// cases expect? It needs no code of Hune. selfcheck.sh runs it with mongosh inside the container.
//
// The rules M1 to M3 below are the rules of 8b.12, written again in plain JavaScript. They are a check of the
// fixture. The check of the program is in the crate hune-source.
// Environment: MGS_PORT, MGS_ADMIN_PASSWORD, MGS_RO_PASSWORD, MGS_RW_PASSWORD, MGS_BAD_PASSWORD,
//   MGS_EXPECT_DOCS (the size of node_load), MGS_EXPECT_BIG (the size of the text of `big`),
//   MGS_EXPECT_VERSION (a prefix, for example 7.0), MGS_EXPECT_NOSCRIPTING (1 or 0), MGS_EXPECT_REPLSET (a name or
//   empty), MGS_SHA1_ONLY (1 for the server that offers SCRAM-SHA-1 only: only root and hune_sha1 can log in there),
//   MGS_PLAIN_ONLY (1 for the server that offers PLAIN only: nobody can log in there, the community server has no
//   PLAIN credential).
// Output: one line "ok      FXnn name" or "FAIL    FXnn name: detail" for each check, and a last line "failed N".

const env = process.env;
const port = env.MGS_PORT;
const uri = "mongodb://localhost:" + port + "/?tls=true&tlsCAFile=%2Ftls%2Fca.pem&directConnection=true";
const sha1Only = env.MGS_SHA1_ONLY === "1";
const plainOnly = env.MGS_PLAIN_ONLY === "1";
let failed = 0;

function check(name, ok, detail) {
  if (ok) {
    print("ok      " + name);
  } else {
    print("FAIL    " + name + (detail === undefined ? "" : ": " + detail));
    failed++;
  }
}
function login(user, password, authdb) {
  const conn = new Mongo(uri);
  conn.getDB(authdb).auth(user, password);
  return conn;
}
// The error code of a statement, or null when it succeeded. A command that answers ok: 0 gives its code.
function codeOf(fn) {
  try {
    const r = fn();
    if (r && r.ok === 0) return r.code;
    return null;
  } catch (e) {
    return e.code === undefined ? "no-code:" + e.message : e.code;
  }
}

const ADMIN = "admin";
const D = ["analytics"];
const M3_ACTIONS = ["changeStream", "collStats", "dbHash", "dbStats", "find", "killCursors", "listCollections",
  "listIndexes", "listSearchIndexes", "planCacheRead"];

// The checks M1, M2 and M3 of 8b.12 on one answer of connectionStatus. It returns the list of failed checks.
function probe(status, user, authdb) {
  const failedChecks = [];
  const info = status.authInfo;
  const users = info.authenticatedUsers;
  if (!(users.length === 1 && users[0].user === user && users[0].db === authdb)) failedChecks.push("M1");
  const roles = info.authenticatedUserRoles;
  const m2 = roles.every((r) => r.role === "read" && D.includes(r.db)) &&
    D.every((d) => roles.some((r) => r.role === "read" && r.db === d));
  if (!m2) failedChecks.push("M2");
  const m3 = info.authenticatedUserPrivileges.every((p) =>
    p.resource.db !== undefined && D.includes(p.resource.db) &&
    p.resource.cluster === undefined && p.resource.anyResource === undefined &&
    p.actions.every((a) => M3_ACTIONS.includes(a)));
  if (!m3) failedChecks.push("M3");
  return failedChecks;
}
function statusOf(conn) {
  return conn.getDB("admin").runCommand({ connectionStatus: 1, showPrivileges: true });
}

if (plainOnly) {
  const c = new Mongo(uri);
  check("FX70 PLAIN-only server: hello works over TLS with no login", c.getDB("admin").hello().isWritablePrimary === true);
  check("FX71 PLAIN-only server: root cannot log in", codeOf(() => login("root", env.MGS_ADMIN_PASSWORD, ADMIN)) !== null);
  print("failed " + failed);
  quit(0);
}

// --- data (as root) ---
{
  const root = login("root", env.MGS_ADMIN_PASSWORD, ADMIN);
  const version = root.getDB("admin").version();
  check("FX60 server version " + version, version.startsWith(env.MGS_EXPECT_VERSION + "."), "expected " + env.MGS_EXPECT_VERSION);
  const expectSet = env.MGS_EXPECT_REPLSET || undefined;
  // The first member has the priority 2. A set of two members can elect the other one at the start: wait for the
  // priority takeover (at most 60 s).
  let hello = root.getDB("admin").hello();
  for (let i = 0; expectSet !== undefined && hello.isWritablePrimary !== true && i < 60; i++) {
    sleep(1000);
    hello = root.getDB("admin").hello();
  }
  check("FX61 replica set name", hello.setName === expectSet, "got " + hello.setName);
  if (expectSet !== undefined) check("FX62 the member is the primary", hello.isWritablePrimary === true);
  const where = codeOf(() => root.getDB("analytics").node_events.find({ $where: "this.code === 1" }).limit(1).toArray());
  if (env.MGS_EXPECT_NOSCRIPTING === "1") {
    check("FX63 --noscripting: $where fails", where !== null, "code " + where);
  } else {
    check("FX63 scripting on: $where runs (MS18)", where === null, "code " + where);
  }
  const plain = codeOf(() => {
    const c = new Mongo("mongodb://localhost:" + port + "/?directConnection=true&serverSelectionTimeoutMS=3000");
    return c.getDB("admin").runCommand({ ping: 1 });
  });
  check("FX64 a connection with no TLS fails", plain !== null);
  const a = root.getDB("analytics");
  const expectDocs = Number(env.MGS_EXPECT_DOCS);
  const expectBig = Number(env.MGS_EXPECT_BIG);
  check("FX01 node_load size", a.node_load.countDocuments({}) === expectDocs,
    "expected " + expectDocs + ", got " + a.node_load.countDocuments({}));
  const last = a.node_load.findOne({ _id: expectDocs - 1 });
  check("FX02 node_load last document", last !== null && last.ts.getTime() === Date.UTC(2026, 0, 1) + (expectDocs - 1) * 1000);
  check("FX03 node_events size", a.node_events.countDocuments({}) === 5000);
  check("FX04 types has 5 documents", a.types.countDocuments({}) === 5);
  const wide = a.types.findOne({ _id: 4 });
  check("FX05 types wide document has 257 fields plus _id", Object.keys(wide).length === 258);
  const big = a.big.findOne({ _id: 1 });
  check("FX06 big document size", big !== null && big.payload.length === expectBig, "payload " + (big && big.payload.length));
  check("FX07 secret has 3 documents", a.secret.countDocuments({}) === 3);
  check("FX08 other.stuff has 3 documents", root.getDB("other").stuff.countDocuments({}) === 3);
  check("FX09 orders has 200 documents", a.orders.countDocuments({}) === 200);
  const hasPlain = root.getDB("admin").getCollection("system.users");
  const cred = (name) => Object.keys(hasPlain.findOne({ user: name, db: "admin" }).credentials).sort().join(",");
  check("FX10 hune_ro has the mechanism SCRAM-SHA-256 only", cred("hune_ro") === "SCRAM-SHA-256", cred("hune_ro"));
  check("FX11 hune_sha1 has the mechanism SCRAM-SHA-1 only", cred("hune_sha1") === "SCRAM-SHA-1", cred("hune_sha1"));
}

// --- the user hune_ro: the contract of 8b.13 (MS08 role side) ---
if (sha1Only) {
  const sha1 = statusOf(login("hune_sha1", env.MGS_LEGACY_PASSWORD, ADMIN));
  check("FX34 hune_sha1 has the contract of hune_ro", probe(sha1, "hune_sha1", ADMIN).length === 0);
  check("FX35 hune_ro cannot log in (SCRAM-SHA-256 only user, SCRAM-SHA-1 only server)",
    codeOf(() => login("hune_ro", env.MGS_RO_PASSWORD, ADMIN)) !== null);
  print("failed " + failed);
  quit(0);
}
{
  const ro = login("hune_ro", env.MGS_RO_PASSWORD, ADMIN);
  const st = statusOf(ro);
  check("FX20 hune_ro: connectionStatus works", st.ok === 1);
  const failedChecks = probe(st, "hune_ro", ADMIN);
  check("FX21 hune_ro passes M1, M2 and M3", failedChecks.length === 0, failedChecks.join(","));
  const roles = st.authInfo.authenticatedUserRoles;
  check("FX22 hune_ro has the one role read on analytics", roles.length === 1 && roles[0].role === "read" &&
    roles[0].db === "analytics", JSON.stringify(roles));
  const actions = new Set();
  st.authInfo.authenticatedUserPrivileges.forEach((p) => p.actions.forEach((x) => actions.add(x)));
  const bad = [...actions].filter((x) => !M3_ACTIONS.includes(x));
  check("FX23 hune_ro holds only read actions", bad.length === 0, bad.join(","));
  const sib = (n) => ro.getDB(n);
  const statements = [
    ["insertOne", () => sib("analytics").node_load.insertOne({ x: 1 })],
    ["updateOne", () => sib("analytics").node_load.updateOne({}, { $set: { x: 1 } })],
    ["deleteMany", () => sib("analytics").node_load.deleteMany({})],
    ["drop", () => sib("analytics").node_load.drop()],
    ["createCollection", () => sib("analytics").createCollection("x")],
    ["createIndex", () => sib("analytics").node_load.createIndex({ x: 1 })],
    ["createUser", () => sib("admin").createUser({ user: "x", pwd: "y", roles: [] })],
    ["$out", () => sib("analytics").node_load.aggregate([{ $out: "x" }]).toArray()],
    ["$merge", () => sib("analytics").node_load.aggregate([{ $merge: { into: "x" } }]).toArray()],
    // The text of 8b.13 has `$currentOp: {}`. That form lists the own operations of the user and needs no
    // right, so it succeeds. The form with `allUsers: true` needs the action inprog, and it fails.
    ["$currentOp allUsers", () => sib("admin").aggregate([{ $currentOp: { allUsers: true } }]).toArray()],
    ["serverStatus", () => sib("admin").runCommand({ serverStatus: 1 })],
    ["getParameter", () => sib("admin").runCommand({ getParameter: 1, javascriptEnabled: 1 })],
    ["other.stuff.find", () => sib("other").stuff.find({}).toArray()],
  ];
  check("FX36 hune_ro: $currentOp with {} lists its own operations (8b.13 text says code 13)",
    codeOf(() => sib("admin").aggregate([{ $currentOp: {} }]).toArray()) === null);
  statements.forEach(([name, fn]) => {
    const c = codeOf(fn);
    check("FX30 hune_ro: " + name + " fails with code 13", c === 13, "code " + c);
  });
  check("FX31 hune_ro reads the collection secret", sib("analytics").secret.countDocuments({}) === 3);
  check("FX32 hune_ro reads node_load", sib("analytics").node_load.findOne({ _id: 0 }) !== null);
  check("FX33 the insert of hune_ro left no document", login("root", env.MGS_ADMIN_PASSWORD, ADMIN)
    .getDB("analytics").node_load.countDocuments({ x: 1 }) === 0);
  const sha1 = statusOf(login("hune_sha1", env.MGS_LEGACY_PASSWORD, ADMIN));
  check("FX34 hune_sha1 has the contract of hune_ro", probe(sha1, "hune_sha1", ADMIN).length === 0);
}

// --- the deviant users: each one breaks its rule (MS11) ---
{
  const rw = statusOf(login("hune_rw", env.MGS_RW_PASSWORD, ADMIN));
  const rwFailed = probe(rw, "hune_rw", ADMIN);
  check("FX40 hune_rw fails M2 and M3", rwFailed.includes("M2") && rwFailed.includes("M3"), rwFailed.join(","));
  const bad = [
    ["hune_bad_readwrite", ADMIN, "M2"],
    ["hune_bad_readany", ADMIN, "M2"],
    ["hune_bad_monitor", ADMIN, "M2"],
    ["hune_bad_extradb", ADMIN, "M2"],
    ["hune_bad_insert", ADMIN, "M3"],
    ["hune_bad_anyres", ADMIN, "M3"],
    ["hune_bad_cluster", ADMIN, "M3"],
    ["hune_bad_authdb", "other", "M1"],
  ];
  bad.forEach(([name, authdb, rule]) => {
    const st = statusOf(login(name, env.MGS_BAD_PASSWORD, authdb));
    const f = probe(st, name, ADMIN);
    check("FX41 " + name + " fails " + rule, f.includes(rule), "failed checks: " + f.join(","));
  });
  // M3 on its own: the M3 users hold read plus one more role, so M2 fails too, and M1 passes.
  ["hune_bad_insert", "hune_bad_anyres", "hune_bad_cluster"].forEach((name) => {
    const f = probe(statusOf(login(name, env.MGS_BAD_PASSWORD, ADMIN)), name, ADMIN);
    check("FX42 " + name + " passes M1", !f.includes("M1"), f.join(","));
  });
  // The write of hune_rw works: this is the connection side of MS09 (the role allows it).
  const w = codeOf(() => login("hune_rw", env.MGS_RW_PASSWORD, ADMIN).getDB("analytics").scratch.insertOne({ x: 1 }));
  check("FX43 hune_rw can write (so the client guard of MS09 is the only barrier)", w === null, "code " + w);
  login("root", env.MGS_ADMIN_PASSWORD, ADMIN).getDB("analytics").scratch.drop();
}

print("failed " + failed);
